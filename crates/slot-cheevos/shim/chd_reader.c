/* A disc reader for rcheevos that also reads CHD images (libchdr), so a PlayStation game kept
 * as .chd is identified like one kept as .cue/.bin. Anything else goes to rcheevos' own reader. */
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#include "rc_hash.h"
#include "libchdr/chd.h"
#include "libchdr/cdrom.h"

typedef struct {
  chd_file* chd;
  const chd_header* header;
  uint8_t* hunk;
  uint32_t hunk_at;           /* which hunk `hunk` holds; -1 for none */
  uint32_t frames_per_hunk;
  uint32_t file_frame;        /* the track's first sector (index 1), in the file's frames */
  uint32_t first_sector;      /* the same sector, as an address on the disc */
  uint32_t data_offset;       /* where in a frame the sector's data starts */
  uint32_t data_size;
} chd_track;

typedef struct {
  int chd;                    /* which reader the handle is for */
  void* inner;
} track_handle;

static struct rc_hash_cdreader g_default;

static int is_chd(const char* path) {
  const char* dot = path ? strrchr(path, '.') : NULL;
  return dot && (dot[1] | 0x20) == 'c' && (dot[2] | 0x20) == 'h' && (dot[3] | 0x20) == 'd' && dot[4] == '\0';
}

/* The data in a frame of each kind of track: where it starts, and how much there is. */
static void data_of(const char* type, uint32_t* offset, uint32_t* size) {
  *offset = 0;
  *size = 2048;
  if (strcmp(type, "MODE1_RAW") == 0 || strcmp(type, "MODE1/2352") == 0)
    *offset = 16;
  else if (strcmp(type, "MODE2_RAW") == 0 || strcmp(type, "MODE2/2352") == 0 || strcmp(type, "CDI/2352") == 0)
    *offset = 24;
  else if (strcmp(type, "MODE2") == 0 || strcmp(type, "MODE2/2336") == 0)
    *offset = 8;
  else if (strcmp(type, "AUDIO") == 0)
    *size = 2352;
}

static void* chd_open_track(const char* path, uint32_t track) {
  chd_file* chd = NULL;
  chd_track* t;
  uint32_t file_frame = 0, lba = 0, index, largest = 0;
  int found = 0;

  if (chd_open(path, CHD_OPEN_READ, NULL, &chd) != CHDERR_NONE)
    return NULL;
  t = (chd_track*)calloc(1, sizeof(*t));
  if (!t) {
    chd_close(chd);
    return NULL;
  }

  for (index = 0;; ++index) {
    char meta[256], type[64] = "", subtype[32] = "", pgtype[32] = "", pgsub[32] = "";
    int number = 0, frames = 0, pregap = 0, postgap = 0;
    uint32_t meta_size = 0, stored_pregap, offset, size;
    int pick = 0;

    if (chd_get_metadata(chd, CDROM_TRACK_METADATA2_TAG, index, meta, sizeof(meta), &meta_size, NULL, NULL) == CHDERR_NONE)
      sscanf(meta, CDROM_TRACK_METADATA2_FORMAT, &number, type, subtype, &frames, &pregap, pgtype, pgsub, &postgap);
    else if (chd_get_metadata(chd, CDROM_TRACK_METADATA_TAG, index, meta, sizeof(meta), &meta_size, NULL, NULL) == CHDERR_NONE)
      sscanf(meta, CDROM_TRACK_METADATA_FORMAT, &number, type, subtype, &frames);
    else
      break;

    /* A pregap is in the file when its type starts with V; the frames count includes it. */
    stored_pregap = (pgtype[0] == 'V') ? (uint32_t)pregap : 0;
    data_of(type, &offset, &size);
    if (track == RC_HASH_CDTRACK_FIRST_DATA)
      pick = !found && strcmp(type, "AUDIO") != 0;
    else if (track == RC_HASH_CDTRACK_LAST)
      pick = 1;
    else if (track == RC_HASH_CDTRACK_LARGEST)
      pick = (uint32_t)frames > largest;
    else
      pick = (uint32_t)number == track;

    lba += (uint32_t)pregap - stored_pregap;
    if (pick) {
      found = 1;
      largest = (uint32_t)frames;
      t->file_frame = file_frame + stored_pregap;
      t->first_sector = lba + stored_pregap;
      t->data_offset = offset;
      t->data_size = size;
    }
    lba += (uint32_t)frames;
    /* Each track is padded in the file to a whole number of four frames. */
    file_frame += ((uint32_t)frames + CD_TRACK_PADDING - 1) / CD_TRACK_PADDING * CD_TRACK_PADDING;
  }

  t->chd = chd;
  t->header = chd_get_header(chd);
  if (!found || !t->header || t->header->hunkbytes < CD_FRAME_SIZE) {
    chd_close(chd);
    free(t);
    return NULL;
  }
  t->frames_per_hunk = t->header->hunkbytes / CD_FRAME_SIZE;
  t->hunk = (uint8_t*)malloc(t->header->hunkbytes);
  t->hunk_at = (uint32_t)-1;
  if (!t->hunk) {
    chd_close(chd);
    free(t);
    return NULL;
  }
  return t;
}

static size_t chd_read_sector(chd_track* t, uint32_t sector, void* buffer, size_t requested) {
  uint8_t* out = (uint8_t*)buffer;
  size_t total = 0;
  uint32_t frame;

  if (sector < t->first_sector)
    return 0;
  frame = t->file_frame + (sector - t->first_sector);
  while (requested > 0) {
    uint32_t hunk = frame / t->frames_per_hunk;
    size_t take = requested < t->data_size ? requested : t->data_size;
    if (hunk != t->hunk_at) {
      if (chd_read(t->chd, hunk, t->hunk) != CHDERR_NONE)
        return total;
      t->hunk_at = hunk;
    }
    memcpy(out, t->hunk + (frame % t->frames_per_hunk) * CD_FRAME_SIZE + t->data_offset, take);
    out += take;
    total += take;
    requested -= take;
    ++frame;
  }
  return total;
}

static void* reader_open_track(const char* path, uint32_t track) {
  track_handle* h = (track_handle*)calloc(1, sizeof(*h));
  if (!h)
    return NULL;
  h->chd = is_chd(path);
  h->inner = h->chd ? chd_open_track(path, track) : g_default.open_track(path, track);
  if (!h->inner) {
    free(h);
    return NULL;
  }
  return h;
}

static void* reader_open_track_iterator(const char* path, uint32_t track, const rc_hash_iterator_t* iterator) {
  track_handle* h;
  if (!g_default.open_track_iterator || is_chd(path))
    return reader_open_track(path, track);
  h = (track_handle*)calloc(1, sizeof(*h));
  if (!h)
    return NULL;
  h->inner = g_default.open_track_iterator(path, track, iterator);
  if (!h->inner) {
    free(h);
    return NULL;
  }
  return h;
}

static size_t reader_read_sector(void* handle, uint32_t sector, void* buffer, size_t requested) {
  track_handle* h = (track_handle*)handle;
  if (!h)
    return 0;
  return h->chd ? chd_read_sector((chd_track*)h->inner, sector, buffer, requested)
                : g_default.read_sector(h->inner, sector, buffer, requested);
}

static void reader_close_track(void* handle) {
  track_handle* h = (track_handle*)handle;
  if (!h)
    return;
  if (h->chd) {
    chd_track* t = (chd_track*)h->inner;
    chd_close(t->chd);
    free(t->hunk);
    free(t);
  }
  else {
    g_default.close_track(h->inner);
  }
  free(h);
}

static uint32_t reader_first_track_sector(void* handle) {
  track_handle* h = (track_handle*)handle;
  if (!h)
    return 0;
  return h->chd ? ((chd_track*)h->inner)->first_sector : g_default.first_track_sector(h->inner);
}

/* Installs the reader for every hash rcheevos makes from now on. */
void slot_cheevos_use_chd_reader(void) {
  struct rc_hash_cdreader reader;
  rc_hash_get_default_cdreader(&g_default);
  reader.open_track = reader_open_track;
  reader.read_sector = reader_read_sector;
  reader.close_track = reader_close_track;
  reader.first_track_sector = reader_first_track_sector;
  reader.open_track_iterator = reader_open_track_iterator;
  rc_hash_init_custom_cdreader(&reader);
}
