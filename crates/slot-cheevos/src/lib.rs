//! RetroAchievements for slot: rcheevos' rc_client behind a small safe API.
//!
//! The host supplies HTTP (`Http`), the core's memory (`set_memory`) and calls `frame`
//! once per emulated frame. What happens comes back from `take` as `Notice`s.
//! Softcore only: slot is not an approved hardcore emulator.
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

type HttpFn = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *const c_char, *mut c_void);
type EventFn = unsafe extern "C" fn(*mut c_void, c_int, *const c_char, *const c_char, c_uint, *const c_char);
type DoneFn = unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char, *const c_char);
type EachFn = unsafe extern "C" fn(*mut c_void, *const c_char);
type MemoryFn = unsafe extern "C" fn(*mut c_void, c_uint, *mut *mut u8, *mut usize);

extern "C" {
    fn slot_cheevos_new(host: *mut c_void, http: HttpFn, event: EventFn, done: DoneFn, memory: MemoryFn) -> *mut Raw;
    fn slot_cheevos_free(s: *mut Raw);
    fn slot_cheevos_respond(call: *mut c_void, body: *const c_char, length: usize, status: c_int);
    fn slot_cheevos_login(s: *mut Raw, user: *const c_char, token: *const c_char);
    fn slot_cheevos_set_memory(s: *mut Raw, descriptors: *const c_void, count: c_uint, console: c_uint) -> c_int;
    fn slot_cheevos_load_game(s: *mut Raw, path: *const c_char, console: c_uint);
    fn slot_cheevos_frame(s: *mut Raw);
    fn slot_cheevos_idle(s: *mut Raw);
    fn slot_cheevos_reset(s: *mut Raw);
    fn slot_cheevos_unload(s: *mut Raw);
    fn slot_cheevos_badges(s: *mut Raw, each: EachFn, ctx: *mut c_void) -> c_uint;
    fn slot_cheevos_version() -> *const c_char;
    fn slot_cheevos_console_gba() -> c_uint;
    fn slot_cheevos_console_gb() -> c_uint;
    fn slot_cheevos_console_gbc() -> c_uint;
    fn slot_cheevos_console_nes() -> c_uint;
    fn slot_cheevos_console_snes() -> c_uint;
}

/// rcheevos' version, for the User-Agent RetroAchievements asks clients to send.
pub fn version() -> &'static str {
    unsafe { CStr::from_ptr(slot_cheevos_version()) }.to_str().unwrap_or("?")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Console {
    Gba,
    Gb,
    Gbc,
    Nes,
    Snes,
}

impl Console {
    fn id(self) -> c_uint {
        unsafe {
            match self {
                Console::Gba => slot_cheevos_console_gba(),
                Console::Gb => slot_cheevos_console_gb(),
                Console::Gbc => slot_cheevos_console_gbc(),
                Console::Nes => slot_cheevos_console_nes(),
                Console::Snes => slot_cheevos_console_snes(),
            }
        }
    }
}

/// One request from rcheevos. `post` set means POST with that body.
pub struct HttpRequest {
    pub url: String,
    pub post: Option<String>,
    pub content_type: Option<String>,
}

/// The host's HTTP(S). Called on a worker thread; may block.
pub trait Http: Send + Sync + 'static {
    /// The status and body, or why there is none.
    fn send(&self, request: &HttpRequest) -> Result<(u16, Vec<u8>), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    LoggedIn(Result<(), String>),
    /// "Title: 3 of 40 achievements" and the game's icon URL, or why the game has none.
    GameLoaded(Result<(String, String), String>),
    /// `badge`: the achievement's image URL ("" if none).
    Unlocked { title: String, description: String, points: u32, badge: String },
    GameCompleted { title: String, badge: String },
    LeaderboardStarted { title: String, description: String },
    LeaderboardFailed { title: String },
    LeaderboardSubmitted { title: String, score: String },
    ServerError { api: String, message: String },
    /// Unlocks are waiting to be sent.
    Disconnected,
    /// Everything waiting has been sent.
    Reconnected,
}

struct State {
    raw: usize,
    inflight: usize,
    closing: bool,
}

/// What the C callbacks reach through their host pointer.
struct Shared {
    http: Arc<dyn Http>,
    notices: Mutex<Vec<Notice>>,
    state: Mutex<State>,
}

impl Shared {
    fn push(&self, notice: Notice) {
        self.notices.lock().unwrap_or_else(|e| e.into_inner()).push(notice);
    }

    /// Frees the client once nothing is in flight and its owner has let go.
    fn finish(shared: *const Shared) {
        let s = unsafe { &*shared };
        let raw = {
            let mut st = s.state.lock().unwrap_or_else(|e| e.into_inner());
            if !st.closing || st.inflight > 0 || st.raw == 0 {
                return;
            }
            std::mem::replace(&mut st.raw, 0)
        };
        unsafe {
            slot_cheevos_free(raw as *mut Raw);
            drop(Arc::from_raw(shared));
        }
    }
}

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

unsafe extern "C" fn on_http(host: *mut c_void, url: *const c_char, post: *const c_char, kind: *const c_char, call: *mut c_void) {
    let shared = host as *const Shared;
    let request = HttpRequest {
        url: text(url),
        post: (!post.is_null()).then(|| text(post)),
        content_type: (!kind.is_null()).then(|| text(kind)),
    };
    (*shared).state.lock().unwrap_or_else(|e| e.into_inner()).inflight += 1;
    let (shared, call) = (shared as usize, call as usize);
    let spawned = std::thread::Builder::new().name("slot-cheevos-http".into()).spawn(move || {
        let s = unsafe { &*(shared as *const Shared) };
        let reply = s.http.send(&request);
        if let Err(e) = &reply {
            eprintln!("slot: cheevos: {}: {e}", request.url.split('?').next().unwrap_or(""));
        }
        let (status, body) = reply.unwrap_or((0, Vec::new()));
        let status = if status == 0 { -1 } else { status as c_int };
        unsafe { slot_cheevos_respond(call as *mut c_void, body.as_ptr() as *const c_char, body.len(), status) };
        s.state.lock().unwrap_or_else(|e| e.into_inner()).inflight -= 1;
        Shared::finish(shared as *const Shared);
    });
    if spawned.is_err() {
        (*(shared as *const Shared)).state.lock().unwrap_or_else(|e| e.into_inner()).inflight -= 1;
        slot_cheevos_respond(call as *mut c_void, std::ptr::null(), 0, -1);
    }
}

unsafe extern "C" fn on_event(host: *mut c_void, kind: c_int, title: *const c_char, detail: *const c_char, points: c_uint, badge: *const c_char) {
    let (title, detail, badge) = (text(title), text(detail), text(badge));
    let notice = match kind {
        1 => Notice::Unlocked { title, description: detail, points, badge },
        2 => Notice::GameCompleted { title, badge },
        3 => Notice::ServerError { api: title, message: detail },
        4 => Notice::Disconnected,
        5 => Notice::Reconnected,
        6 => Notice::LeaderboardStarted { title, description: detail },
        7 => Notice::LeaderboardFailed { title },
        8 => Notice::LeaderboardSubmitted { title, score: detail },
        _ => return,
    };
    (*(host as *const Shared)).push(notice);
}

unsafe extern "C" fn on_done(host: *mut c_void, op: c_int, ok: c_int, message: *const c_char, badge: *const c_char) {
    let message = text(message);
    let notice = match (op, ok != 0) {
        (1, true) => Notice::LoggedIn(Ok(())),
        (1, false) => Notice::LoggedIn(Err(message)),
        (_, true) => Notice::GameLoaded(Ok((message, text(badge)))),
        (_, false) => Notice::GameLoaded(Err(message)),
    };
    (*(host as *const Shared)).push(notice);
}

/// Answers retro_get_memory_data/size for a libretro memory id.
type MemoryQuery = dyn FnMut(u32) -> Option<(*mut u8, usize)>;
type CoreMemory<'a> = &'a mut (dyn FnMut(u32) -> Option<(*mut u8, usize)> + 'a);
thread_local! {
    /// The memory callback of the `set_memory` call in progress, on this thread.
    static MEMORY: RefCell<Option<*mut MemoryQuery>> = const { RefCell::new(None) };
}

unsafe extern "C" fn on_memory(_host: *mut c_void, id: c_uint, data: *mut *mut u8, size: *mut usize) {
    MEMORY.with(|m| {
        if let Some(f) = *m.borrow() {
            if let Some((p, n)) = (*f)(id) {
                *data = p;
                *size = n;
            }
        }
    });
}

pub struct Cheevos {
    raw: *mut Raw,
    shared: *const Shared,
}

// rc_client locks itself; its memory reads only happen inside `frame`, on the caller.
unsafe impl Send for Cheevos {}

impl Cheevos {
    pub fn new(http: Arc<dyn Http>) -> Option<Cheevos> {
        let shared = Arc::into_raw(Arc::new(Shared {
            http,
            notices: Mutex::new(Vec::new()),
            state: Mutex::new(State { raw: 0, inflight: 0, closing: false }),
        }));
        let raw = unsafe { slot_cheevos_new(shared as *mut c_void, on_http, on_event, on_done, on_memory) };
        if raw.is_null() {
            unsafe { drop(Arc::from_raw(shared)) };
            return None;
        }
        unsafe { (*shared).state.lock().unwrap_or_else(|e| e.into_inner()).raw = raw as usize };
        Some(Cheevos { raw, shared })
    }

    pub fn login(&self, user: &str, token: &str) {
        let (Ok(user), Ok(token)) = (CString::new(user), CString::new(token)) else {
            unsafe { (*self.shared).push(Notice::LoggedIn(Err("bad user name or token".into()))) };
            return;
        };
        unsafe { slot_cheevos_login(self.raw, user.as_ptr(), token.as_ptr()) };
    }

    /// Points rcheevos at the core's memory: `descriptors`/`count` are the core's
    /// SET_MEMORY_MAPS (null/0 if none), `memory` answers retro_get_memory_data/size.
    /// The bytes mapped, 0 if none could be.
    ///
    /// # Safety
    /// `descriptors` is null or points to `count` retro_memory_descriptors, and they and
    /// the memory they and `memory` point at stay valid while frames run.
    pub unsafe fn set_memory(&mut self, descriptors: *const c_void, count: u32, console: Console, memory: CoreMemory) -> usize {
        // Only reached from on_memory while slot_cheevos_set_memory runs below.
        let memory: *mut (dyn FnMut(u32) -> Option<(*mut u8, usize)> + '_) = memory;
        let memory: *mut MemoryQuery = unsafe { std::mem::transmute(memory) };
        MEMORY.with(|m| *m.borrow_mut() = Some(memory));
        let mapped = unsafe { slot_cheevos_set_memory(self.raw, descriptors, count, console.id()) };
        MEMORY.with(|m| *m.borrow_mut() = None);
        mapped.max(0) as usize
    }

    /// Hashes the ROM and fetches its achievements; answered by Notice::GameLoaded.
    pub fn load_game(&self, rom: &Path, console: Console) {
        let Ok(path) = CString::new(rom.to_string_lossy().as_bytes()) else {
            unsafe { (*self.shared).push(Notice::GameLoaded(Err("bad ROM path".into()))) };
            return;
        };
        unsafe { slot_cheevos_load_game(self.raw, path.as_ptr(), console.id()) };
    }

    /// Once per emulated frame, after the core ran it.
    pub fn frame(&self) {
        unsafe { slot_cheevos_frame(self.raw) }
    }

    /// While paused or in menus, so pings and retries go on.
    pub fn idle(&self) {
        unsafe { slot_cheevos_idle(self.raw) }
    }

    /// After loading a save state, rewinding or resetting.
    pub fn reset(&self) {
        unsafe { slot_cheevos_reset(self.raw) }
    }

    pub fn unload(&self) {
        unsafe { slot_cheevos_unload(self.raw) }
    }

    /// Every achievement's badge URL for the loaded game, to fetch before they are needed.
    pub fn badge_urls(&self) -> Vec<String> {
        unsafe extern "C" fn each(ctx: *mut c_void, url: *const c_char) {
            (*(ctx as *mut Vec<String>)).push(text(url));
        }
        let mut urls = Vec::new();
        unsafe { slot_cheevos_badges(self.raw, each, &mut urls as *mut Vec<String> as *mut c_void) };
        urls
    }

    pub fn take(&self) -> Vec<Notice> {
        std::mem::take(&mut *unsafe { &*self.shared }.notices.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Drop for Cheevos {
    fn drop(&mut self) {
        unsafe { (*self.shared).state.lock().unwrap_or_else(|e| e.into_inner()).closing = true };
        Shared::finish(self.shared);
    }
}
