/* slot's thin layer over rcheevos' rc_client: plain functions and plain callbacks, so the
 * Rust side never mirrors rcheevos' structs. One instance per running game session.
 *
 * Threading: rc_client locks itself; slot_cheevos_respond may be called from any thread.
 * Everything else, and every memory read, happens on the emulator thread. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "rc_client.h"
#include "rc_consoles.h"
#include "rc_libretro.h"
#include "rc_version.h"

typedef void (*slot_http_fn)(void* host, const char* url, const char* post_data,
                             const char* content_type, void* call);
/* `badge`: the image URL for the achievement (or game), "" if none. */
typedef void (*slot_event_fn)(void* host, int kind, const char* title, const char* detail,
                              unsigned points, const char* badge);
typedef void (*slot_done_fn)(void* host, int op, int ok, const char* message, const char* badge);
typedef void (*slot_each_fn)(void* ctx, const char* url);
typedef void (*slot_memory_fn)(void* host, unsigned id, unsigned char** data, size_t* size);

enum { SLOT_OP_LOGIN = 1, SLOT_OP_GAME = 2 };
enum {
  SLOT_EVENT_UNLOCK = 1,
  SLOT_EVENT_GAME_COMPLETED = 2,
  SLOT_EVENT_SERVER_ERROR = 3,
  SLOT_EVENT_DISCONNECTED = 4,
  SLOT_EVENT_RECONNECTED = 5,
  SLOT_EVENT_LEADERBOARD_STARTED = 6,
  SLOT_EVENT_LEADERBOARD_FAILED = 7,
  SLOT_EVENT_LEADERBOARD_SUBMITTED = 8
};

typedef struct slot_cheevos {
  rc_client_t* client;
  void* host;
  slot_http_fn http;
  slot_event_fn event;
  slot_done_fn done;
  slot_memory_fn memory;
  rc_libretro_memory_regions_t regions;
  int has_regions;
} slot_cheevos;

typedef struct slot_call {
  rc_client_server_callback_t callback;
  void* callback_data;
} slot_call;

/* rc_libretro asks for core memory through a callback with no user pointer; the memory
 * map is only built on the emulator thread, so a thread-local is enough to reach it. */
static __thread slot_cheevos* g_building;

static void get_core_memory(uint32_t id, rc_libretro_core_memory_info_t* info) {
  info->data = NULL;
  info->size = 0;
  if (g_building && g_building->memory)
    g_building->memory(g_building->host, id, &info->data, &info->size);
}

static uint32_t read_memory(uint32_t address, uint8_t* buffer, uint32_t num_bytes, rc_client_t* client) {
  slot_cheevos* s = (slot_cheevos*)rc_client_get_userdata(client);
  if (!s || !s->has_regions)
    return 0;
  return rc_libretro_memory_read(&s->regions, address, buffer, num_bytes);
}

static void server_call(const rc_api_request_t* request, rc_client_server_callback_t callback,
                        void* callback_data, rc_client_t* client) {
  slot_cheevos* s = (slot_cheevos*)rc_client_get_userdata(client);
  slot_call* call = (slot_call*)malloc(sizeof(slot_call));
  if (!call) {
    rc_api_server_response_t none = { NULL, 0, RC_API_SERVER_RESPONSE_CLIENT_ERROR };
    callback(&none, callback_data);
    return;
  }
  call->callback = callback;
  call->callback_data = callback_data;
  s->http(s->host, request->url, request->post_data, request->content_type, call);
}

/* Hands an HTTP reply (status < 0: none) back to rcheevos and frees the call. */
void slot_cheevos_respond(void* call, const char* body, size_t length, int status) {
  slot_call* c = (slot_call*)call;
  rc_api_server_response_t response;
  response.body = body;
  response.body_length = length;
  response.http_status_code = status < 0 ? RC_API_SERVER_RESPONSE_RETRYABLE_CLIENT_ERROR : status;
  c->callback(&response, c->callback_data);
  free(c);
}

static void log_message(const char* message, const rc_client_t* client) {
  (void)client;
  fprintf(stderr, "slot: cheevos: %s\n", message);
}

/* The unlocked badge's URL, in `buf` if rcheevos has to build it. */
static const char* badge_of(const rc_client_achievement_t* a, char* buf, size_t size) {
  if (a->badge_url && a->badge_url[0])
    return a->badge_url;
  if (rc_client_achievement_get_image_url(a, RC_CLIENT_ACHIEVEMENT_STATE_UNLOCKED, buf, size) == RC_OK)
    return buf;
  return "";
}

static const char* game_badge(const rc_client_game_t* game, char* buf, size_t size) {
  if (!game)
    return "";
  if (game->badge_url && game->badge_url[0])
    return game->badge_url;
  if (rc_client_game_get_image_url(game, buf, size) == RC_OK)
    return buf;
  return "";
}

static void event_handler(const rc_client_event_t* e, rc_client_t* client) {
  slot_cheevos* s = (slot_cheevos*)rc_client_get_userdata(client);
  char url[256];
  if (!s || !s->event)
    return;
  switch (e->type) {
    case RC_CLIENT_EVENT_ACHIEVEMENT_TRIGGERED:
      s->event(s->host, SLOT_EVENT_UNLOCK, e->achievement->title, e->achievement->description,
               e->achievement->points, badge_of(e->achievement, url, sizeof(url)));
      break;
    case RC_CLIENT_EVENT_GAME_COMPLETED: {
      const rc_client_game_t* game = rc_client_get_game_info(client);
      s->event(s->host, SLOT_EVENT_GAME_COMPLETED, game ? game->title : "", "", 0,
               game_badge(game, url, sizeof(url)));
      break;
    }
    case RC_CLIENT_EVENT_SERVER_ERROR:
      s->event(s->host, SLOT_EVENT_SERVER_ERROR, e->server_error->api, e->server_error->error_message, 0, "");
      break;
    case RC_CLIENT_EVENT_DISCONNECTED:
      s->event(s->host, SLOT_EVENT_DISCONNECTED, "", "", 0, "");
      break;
    case RC_CLIENT_EVENT_RECONNECTED:
      s->event(s->host, SLOT_EVENT_RECONNECTED, "", "", 0, "");
      break;
    case RC_CLIENT_EVENT_LEADERBOARD_STARTED:
      s->event(s->host, SLOT_EVENT_LEADERBOARD_STARTED, e->leaderboard->title, e->leaderboard->description, 0, "");
      break;
    case RC_CLIENT_EVENT_LEADERBOARD_FAILED:
      s->event(s->host, SLOT_EVENT_LEADERBOARD_FAILED, e->leaderboard->title, "", 0, "");
      break;
    case RC_CLIENT_EVENT_LEADERBOARD_SUBMITTED:
      s->event(s->host, SLOT_EVENT_LEADERBOARD_SUBMITTED, e->leaderboard->title,
               e->leaderboard->tracker_value, 0, "");
      break;
    default:
      break;
  }
}

void slot_cheevos_use_chd_reader(void);

slot_cheevos* slot_cheevos_new(void* host, slot_http_fn http, slot_event_fn event, slot_done_fn done,
                               slot_memory_fn memory) {
  slot_cheevos* s = (slot_cheevos*)calloc(1, sizeof(slot_cheevos));
  /* PlayStation discs kept as .chd are read for their hash as .cue/.bin ones are. */
  slot_cheevos_use_chd_reader();
  if (!s)
    return NULL;
  s->host = host;
  s->http = http;
  s->event = event;
  s->done = done;
  s->memory = memory;
  s->client = rc_client_create(read_memory, server_call);
  if (!s->client) {
    free(s);
    return NULL;
  }
  rc_client_set_userdata(s->client, s);
  rc_client_enable_logging(s->client, RC_CLIENT_LOG_LEVEL_INFO, log_message);
  rc_client_set_event_handler(s->client, event_handler);
  /* Softcore: slot is not an approved hardcore emulator. */
  rc_client_set_hardcore_enabled(s->client, 0);
  return s;
}

void slot_cheevos_free(slot_cheevos* s) {
  if (!s)
    return;
  rc_client_destroy(s->client);
  if (s->has_regions)
    rc_libretro_memory_destroy(&s->regions);
  free(s);
}

static void login_done(int result, const char* error_message, rc_client_t* client, void* userdata) {
  slot_cheevos* s = (slot_cheevos*)userdata;
  (void)client;
  s->done(s->host, SLOT_OP_LOGIN, result == RC_OK, error_message ? error_message : "", "");
}

void slot_cheevos_login(slot_cheevos* s, const char* user, const char* token) {
  rc_client_begin_login_with_token(s->client, user, token, login_done, s);
}

/* Maps console addresses onto the core's memory: `descriptors` is the core's
 * retro_memory_descriptor array from SET_MEMORY_MAPS (may be NULL). */
int slot_cheevos_set_memory(slot_cheevos* s, const void* descriptors, unsigned count, unsigned console) {
  struct retro_memory_map map;
  map.descriptors = (const struct retro_memory_descriptor*)descriptors;
  map.num_descriptors = count;
  if (s->has_regions)
    rc_libretro_memory_destroy(&s->regions);
  g_building = s;
  s->has_regions = rc_libretro_memory_init(&s->regions, descriptors ? &map : NULL, get_core_memory, console);
  g_building = NULL;
  return s->has_regions ? (int)s->regions.total_size : 0;
}

static void game_done(int result, const char* error_message, rc_client_t* client, void* userdata) {
  slot_cheevos* s = (slot_cheevos*)userdata;
  char message[256];
  char url[256] = "";
  const char* badge = "";
  (void)client;
  if (result != RC_OK) {
    s->done(s->host, SLOT_OP_GAME, 0, error_message ? error_message : rc_error_str(result), "");
    return;
  }
  {
    const rc_client_game_t* game = rc_client_get_game_info(s->client);
    badge = game_badge(game, url, sizeof(url));
    rc_client_user_game_summary_t sum;
    memset(&sum, 0, sizeof(sum));
    rc_client_get_user_game_summary(s->client, &sum);
    if (sum.num_core_achievements == 0)
      snprintf(message, sizeof(message), "%s has no achievements yet", game ? game->title : "This game");
    else
      snprintf(message, sizeof(message), "%s: %u of %u achievements", game ? game->title : "",
               sum.num_unlocked_achievements, sum.num_core_achievements);
  }
  s->done(s->host, SLOT_OP_GAME, 1, message, badge);
}

/* Every core achievement's unlocked badge URL for the loaded game, to fetch ahead. */
unsigned slot_cheevos_badges(slot_cheevos* s, slot_each_fn each, void* ctx) {
  char url[256];
  unsigned n = 0, b, a;
  rc_client_achievement_list_t* list = rc_client_create_achievement_list(
      s->client, RC_CLIENT_ACHIEVEMENT_CATEGORY_CORE, RC_CLIENT_ACHIEVEMENT_LIST_GROUPING_LOCK_STATE);
  if (!list)
    return 0;
  for (b = 0; b < list->num_buckets; b++) {
    for (a = 0; a < list->buckets[b].num_achievements; a++) {
      const char* badge = badge_of(list->buckets[b].achievements[a], url, sizeof(url));
      if (badge[0]) {
        each(ctx, badge);
        n++;
      }
    }
  }
  rc_client_destroy_achievement_list(list);
  return n;
}

void slot_cheevos_load_game(slot_cheevos* s, const char* path, unsigned console) {
  rc_client_begin_identify_and_load_game(s->client, console, path, NULL, 0, game_done, s);
}

void slot_cheevos_frame(slot_cheevos* s) { rc_client_do_frame(s->client); }
void slot_cheevos_idle(slot_cheevos* s) { rc_client_idle(s->client); }
/* After a save state, a rewind or a reset: start every achievement's tracking afresh. */
void slot_cheevos_reset(slot_cheevos* s) { rc_client_reset(s->client); }
void slot_cheevos_unload(slot_cheevos* s) { rc_client_unload_game(s->client); }
const char* slot_cheevos_version(void) { return RCHEEVOS_VERSION_STRING; }

/* rcheevos' console ids for slot's platforms. */
unsigned slot_cheevos_console_gba(void) { return RC_CONSOLE_GAMEBOY_ADVANCE; }
unsigned slot_cheevos_console_gb(void) { return RC_CONSOLE_GAMEBOY; }
unsigned slot_cheevos_console_gbc(void) { return RC_CONSOLE_GAMEBOY_COLOR; }
unsigned slot_cheevos_console_nes(void) { return RC_CONSOLE_NINTENDO; }
unsigned slot_cheevos_console_snes(void) { return RC_CONSOLE_SUPER_NINTENDO; }
unsigned slot_cheevos_console_ps1(void) { return RC_CONSOLE_PLAYSTATION; }
