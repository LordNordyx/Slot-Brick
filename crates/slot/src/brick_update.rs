//! HOME → Update: checks for a newer Slot, shows what is new in it, and installs it.
use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, mpsc, Arc};
use std::thread;

use slot_input::{Action, Btn};

use crate::update::{self, Channel, Installed, Manifest, Offer, Progress};

/// Below this, unplugged, an update is not started: a Brick that dies halfway through the
/// swap has to be put right from a computer.
const LOW_BATTERY: u8 = 20;

/// Presses that show the channel row, for those testing Dev builds.
const CODE: [Btn; 10] = [Btn::Up, Btn::Up, Btn::Down, Btn::Down, Btn::Left, Btn::Right, Btn::Left, Btn::Right, Btn::B, Btn::A];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Row {
    Check,
    Install,
    Channel,
}

struct Installing {
    /// `Some`: the channel has moved on since the check, so that newer one is shown instead.
    rx: mpsc::Receiver<Result<Option<Manifest>, String>>,
    progress: Arc<Progress>,
    size: u64,
    /// The tenth of a MB last shown, so the screen is redrawn only when that changes.
    shown: u64,
}

pub struct UpdateScreen {
    root: PathBuf,
    base: String,
    installed: Installed,
    channel: Channel,
    unlocked: bool,
    row: usize,
    pub status: String,
    found: Option<(Manifest, Offer)>,
    check: Option<mpsc::Receiver<Result<Manifest, String>>>,
    installing: Option<Installing>,
    armed: bool,
    pressed: Vec<Btn>,
    /// Percent and whether it is charging, as the app last read it.
    pub battery: Option<(u8, bool)>,
    restart: bool,
    /// How far the notes are scrolled (L1 / R1), in lines, and the furthest they can go, as the
    /// screen last laid them out.
    scroll: usize,
    pub scroll_max: std::cell::Cell<usize>,
    /// What the last check found for the footer's symbol to follow: the newer Slot, or None.
    verdict: Option<Option<Manifest>>,
}

/// Lines L1 / R1 move the notes by.
const SCROLL_STEP: usize = 3;

impl UpdateScreen {
    /// `known`: the newer Slot this boot's check found, shown straight away rather than asked
    /// for again.
    pub fn open(root: &Path, known: Option<Manifest>) -> Self {
        let channel = update::saved_channel(root);
        let mut s = Self::with(root, update::base(), Installed::current(), channel);
        match known.filter(|m| m.channel == channel) {
            Some(m) => {
                s.show(m);
            }
            None => s.start_check(),
        }
        s
    }

    fn with(root: &Path, base: String, installed: Installed, channel: Channel) -> Self {
        Self {
            root: root.to_path_buf(),
            base,
            installed,
            channel,
            unlocked: channel == Channel::Dev,
            row: 0,
            status: String::new(),
            found: None,
            check: None,
            installing: None,
            armed: false,
            pressed: Vec::new(),
            battery: None,
            restart: false,
            scroll: 0,
            scroll_max: std::cell::Cell::new(0),
            verdict: None,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Check];
        if self.found.as_ref().is_some_and(|(_, o)| *o != Offer::UpToDate) {
            rows.push(Row::Install);
        }
        if self.unlocked {
            rows.push(Row::Channel);
        }
        rows
    }

    pub fn busy(&self) -> bool {
        self.check.is_some() || self.installing.is_some()
    }

    /// What the last check said about a newer Slot, once.
    pub fn take_verdict(&mut self) -> Option<Option<Manifest>> {
        self.verdict.take()
    }

    /// Swapped in: Slot should end, for launch.sh to start the new one.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    /// B, or Power, while busy: a check is dropped, a download stopped. Once the download is
    /// checked, unpacking and swapping carry on to the end, as stopping halfway would be worse.
    pub fn cancel(&mut self) {
        if self.check.take().is_some() {
            self.status = "Cancelled".into();
        }
        if let Some(job) = &self.installing {
            if !job.progress.unpacking.load(Ordering::Relaxed) {
                job.progress.cancel.store(true, Ordering::Relaxed);
                self.status = "Cancelling...".into();
            }
        }
    }

    fn start_check(&mut self) {
        let (tx, rx) = mpsc::channel();
        let (base, channel) = (self.base.clone(), self.channel);
        thread::spawn(move || {
            let _ = tx.send(update::check(&base, channel));
        });
        self.check = Some(rx);
        self.found = None;
        self.armed = false;
        self.row = 0;
        self.scroll = 0;
        self.status = "Checking for updates...".into();
    }

    /// True when something on screen changed.
    pub fn poll(&mut self) -> bool {
        if let Some(rx) = &self.check {
            let result = match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => Err("The check stopped".into()),
            };
            self.check = None;
            match result {
                Ok(m) => {
                    let offer = self.show(m.clone());
                    self.verdict = Some((offer == Offer::Newer).then_some(m));
                }
                Err(e) => self.status = e,
            }
            return true;
        }
        let Some(job) = &mut self.installing else { return false };
        match job.rx.try_recv() {
            Ok(Ok(Some(m))) => {
                self.installing = None;
                self.show(m.clone());
                self.verdict = Some(Some(m));
                self.status = "A newer update came out since. Install this one?".into();
                true
            }
            Ok(Ok(None)) => {
                self.installing = None;
                self.status = "Installed. Restarting...".into();
                self.restart = true;
                true
            }
            Ok(Err(e)) => {
                self.installing = None;
                self.status = e;
                true
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.installing = None;
                self.status = "The update stopped".into();
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                let (unpacking, done) = (job.progress.unpacking.load(Ordering::Relaxed), job.progress.downloaded.load(Ordering::Relaxed));
                let at = if unpacking { u64::MAX } else { (done * 10) >> 20 };
                if at == job.shown || job.progress.cancel.load(Ordering::Relaxed) {
                    return false;
                }
                job.shown = at;
                self.status = match unpacking {
                    true => "Installing. Keep the Brick on...".into(),
                    false => format!("Downloading {} of {}", mb(done), mb(job.size)),
                };
                true
            }
        }
    }

    /// What a check found, on screen, with Install picked when there is something to install.
    fn show(&mut self, m: Manifest) -> Offer {
        let offer = update::offer(&self.installed, &m);
        self.status = self.found_line(&m, offer);
        self.found = Some((m, offer));
        if offer != Offer::UpToDate {
            self.row = 1;
        }
        offer
    }

    fn found_line(&self, m: &Manifest, offer: Offer) -> String {
        let have = self.have();
        match (m.channel, offer) {
            (Channel::Release, Offer::Newer) => format!("{} is out. You have {have}", m.version),
            (Channel::Release, Offer::Back) => format!("{} replaces this Dev build ({})", m.version, self.installed.commit),
            (Channel::Release, Offer::UpToDate) => format!("Slot is up to date ({have})"),
            (Channel::Dev, Offer::UpToDate) => format!("This is the latest Dev build ({})", m.short()),
            (Channel::Dev, _) => format!("Dev build {} is ready. It may have bugs", m.short()),
        }
    }

    fn have(&self) -> String {
        match self.installed.channel {
            Channel::Release => self.installed.version.clone(),
            Channel::Dev => format!("Dev build {}", self.installed.commit),
        }
    }

    /// True when the screen should close.
    pub fn input(&mut self, a: Action) -> bool {
        if self.busy() {
            if matches!(a, Action::GbaDown(Btn::B)) {
                self.cancel();
            }
            return false;
        }
        if self.restart {
            return false;
        }
        let btn = match a {
            Action::GbaDown(b) => b,
            Action::Insert => Btn::A,
            Action::ShelfLeft => Btn::Left,
            Action::ShelfRight => Btn::Right,
            _ => return false,
        };
        self.pressed.push(btn);
        if self.pressed.len() > CODE.len() {
            self.pressed.remove(0);
        }
        if self.pressed == CODE {
            self.pressed.clear();
            self.unlocked = true;
            self.status = "Update channel unlocked".into();
            self.armed = false;
            return false;
        }
        // The B in the code: kept, not taken for Back.
        if btn == Btn::B && self.pressed.ends_with(&CODE[..CODE.len() - 1]) {
            return false;
        }
        let rows = self.rows();
        let row = rows.get(self.row).copied().unwrap_or(Row::Check);
        match btn {
            Btn::B if self.armed => {
                self.armed = false;
                self.status = "Not installed".into();
            }
            Btn::B => {
                update::tidy(&update::card_of(&self.root));
                return true;
            }
            Btn::Up => {
                self.row = (self.row + rows.len() - 1) % rows.len();
                self.armed = false;
            }
            Btn::Down => {
                self.row = (self.row + 1) % rows.len();
                self.armed = false;
            }
            Btn::Left | Btn::Right if row == Row::Channel => self.switch_channel(),
            Btn::L1 => self.scroll = self.scroll.min(self.scroll_max.get()).saturating_sub(SCROLL_STEP),
            Btn::R1 => self.scroll = (self.scroll + SCROLL_STEP).min(self.scroll_max.get()),
            Btn::A => match row {
                Row::Check => self.start_check(),
                Row::Install => self.install(),
                Row::Channel => self.switch_channel(),
            },
            _ => {}
        }
        false
    }

    fn switch_channel(&mut self) {
        let to = match self.channel {
            Channel::Release => Channel::Dev,
            Channel::Dev => Channel::Release,
        };
        if let Err(e) = update::save_channel(&self.root, to) {
            self.status = format!("Cannot save the channel: {e}");
            return;
        }
        self.channel = to;
        self.start_check();
        self.row = self.rows().len() - 1;
    }

    fn install(&mut self) {
        let Some((m, _)) = &self.found else { return };
        if let Some((percent, charging)) = self.battery {
            if percent < LOW_BATTERY && !charging {
                self.status = format!("Charge the Brick to {LOW_BATTERY}% or plug it in first");
                return;
            }
        }
        if !self.armed {
            self.armed = true;
            self.status = "Press A again to install. Keep the Brick on until Slot restarts".into();
            return;
        }
        self.armed = false;
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let (base, m, card, shared) = (self.base.clone(), m.clone(), update::card_of(&self.root), progress.clone());
        let size = m.size;
        thread::spawn(move || {
            let latest = update::check(&base, m.channel);
            let _ = tx.send(install_latest(&m, latest, || update::install(&base, &m, &card, &shared)));
        });
        self.installing = Some(Installing { rx, progress, size, shown: u64::MAX - 1 });
        self.status = format!("Downloading 0.0 of {}", mb(size));
    }

    /// The rows, as the screen shows them.
    pub fn lines(&self) -> Vec<String> {
        self.rows()
            .iter()
            .map(|row| match row {
                Row::Check => "CHECK FOR UPDATES".into(),
                Row::Install => match &self.found {
                    Some((m, Offer::Back)) => format!("GO BACK TO {}", m.version.to_uppercase()),
                    Some((m, _)) if m.channel == Channel::Dev => format!("INSTALL DEV BUILD {}", m.short().to_uppercase()),
                    Some((m, _)) => format!("INSTALL {}", m.version.to_uppercase()),
                    None => String::new(),
                },
                Row::Channel => format!(
                    "CHANNEL                    {}",
                    match self.channel {
                        Channel::Release => "RELEASE",
                        Channel::Dev => "DEV BUILD",
                    }
                ),
            })
            .collect()
    }

    pub fn row(&self) -> usize {
        self.row
    }

    pub fn scroll(&self) -> usize {
        self.scroll.min(self.scroll_max.get())
    }

    /// What is installed, and what the update found brings, for under the rows.
    pub fn about(&self) -> (String, Vec<String>) {
        let have = format!("Installed: {}", self.have());
        let notes = match &self.found {
            Some((m, offer)) if *offer != Offer::UpToDate => m.notes.iter().map(|l| plain(l)).collect(),
            _ => Vec::new(),
        };
        (have, notes)
    }

    pub fn legend(&self) -> &'static str {
        match () {
            _ if self.restart => "",
            _ if self.installing.as_ref().is_some_and(|j| j.progress.unpacking.load(Ordering::Relaxed)) => "",
            _ if self.busy() => "B CANCEL",
            _ if self.armed => "B KEEP THIS ONE    A INSTALL",
            _ => "B BACK    A SELECT",
        }
    }
}

/// How long the footer's update check waits before asking again.
pub const RECHECK_MS: u64 = 30 * 60 * 1000;

/// The footer's update symbol. The saved channel is asked for its latest update, as HOME →
/// Update asks it, the first time Wi-Fi is up, then every half hour and each time a game is
/// closed, never during a game. Nothing is downloaded but that; a check that fails waits for the
/// next one.
#[derive(Default)]
pub struct AutoCheck {
    /// When the next check may start, on the app's clock.
    due: u64,
    answer: Option<mpsc::Receiver<Result<Manifest, String>>>,
    /// The newer Slot found, for HOME → Update to show without asking again.
    newer: Option<Manifest>,
}

impl AutoCheck {
    /// Starts a check when one is due and `allowed` (the setting is on, Wi-Fi is up, no game is
    /// running), and takes its answer. True when the symbol is to come or go.
    pub fn poll(&mut self, root: &Path, now: u64, allowed: bool) -> bool {
        if allowed && self.answer.is_none() && now >= self.due {
            let (base, channel) = (update::base(), update::saved_channel(root));
            self.start(now, move || update::check(&base, channel));
        }
        let Some(rx) = &self.answer else { return false };
        let got = match rx.try_recv() {
            Ok(got) => got,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err("The check stopped".into()),
        };
        self.answer = None;
        self.found(got, &Installed::current())
    }

    /// A game was closed: check at the next chance.
    pub fn soon(&mut self) {
        self.due = 0;
    }

    fn start(&mut self, now: u64, check: impl FnOnce() -> Result<Manifest, String> + Send + 'static) {
        self.due = now + RECHECK_MS;
        let (tx, rx) = mpsc::channel();
        let spawned = thread::Builder::new().name("slot-update-check".into()).spawn(move || {
            let _ = tx.send(check());
        });
        match spawned {
            Ok(_) => self.answer = Some(rx),
            Err(e) => eprintln!("slot: update check: {e}"),
        }
    }

    fn found(&mut self, got: Result<Manifest, String>, installed: &Installed) -> bool {
        match got {
            Ok(m) => self.take((update::offer(installed, &m) == Offer::Newer).then_some(m)),
            Err(e) => {
                eprintln!("slot: update check: {e}");
                false
            }
        }
    }

    /// What HOME → Update found, which counts as this half hour's check. True when the symbol
    /// is to come or go.
    pub fn set(&mut self, now: u64, newer: Option<Manifest>) -> bool {
        self.due = now + RECHECK_MS;
        self.answer = None;
        self.take(newer)
    }

    fn take(&mut self, newer: Option<Manifest>) -> bool {
        std::mem::replace(&mut self.newer, newer).is_some() != self.newer.is_some()
    }

    pub fn newer(&self) -> Option<&Manifest> {
        self.newer.as_ref()
    }
}

/// Installs `m` unless the channel has a newer update than it now, as when the check was made
/// a while ago; that one is handed back to be shown instead.
fn install_latest(
    m: &Manifest,
    latest: Result<Manifest, String>,
    install: impl FnOnce() -> Result<(), String>,
) -> Result<Option<Manifest>, String> {
    let latest = latest?;
    if latest != *m {
        return Ok(Some(latest));
    }
    install().map(|()| None)
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1 << 20) as f64)
}

/// A changelog line without its Markdown.
fn plain(line: &str) -> String {
    let line = line.trim();
    let line = line.strip_prefix("- ").map(|l| format!("- {l}")).unwrap_or_else(|| line.to_string());
    line.chars().filter(|c| !matches!(c, '*' | '`')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(dir: &Path) -> UpdateScreen {
        let installed = Installed { version: "v0.43".into(), commit: "1111111".into(), channel: Channel::Release };
        UpdateScreen::with(dir, "file:///nowhere".into(), installed, Channel::Release)
    }

    fn press(s: &mut UpdateScreen, buttons: &[Btn]) -> bool {
        buttons.iter().fold(false, |_, b| s.input(Action::GbaDown(*b)))
    }

    fn found(s: &mut UpdateScreen) {
        let m = Manifest {
            channel: Channel::Release,
            version: "v0.44".into(),
            commit: "abcdef0123".into(),
            zip: "z.zip".into(),
            size: 1,
            sha256: [0; 32],
            notes: vec!["- *New*: `this`.".into()],
        };
        s.found = Some((m, Offer::Newer));
        s.row = 1;
    }

    fn release(version: &str) -> Manifest {
        Manifest {
            channel: Channel::Release,
            version: version.into(),
            commit: "abcdef0123".into(),
            zip: "z.zip".into(),
            size: 1,
            sha256: [0; 32],
            notes: Vec::new(),
        }
    }

    #[test]
    fn no_check_starts_until_it_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = AutoCheck::default();
        assert!(!b.poll(dir.path(), 0, false));
        assert!(b.answer.is_none());
    }

    #[test]
    fn the_symbol_shows_only_for_a_newer_slot_and_goes_once_up_to_date() {
        let installed = Installed { version: "v0.43".into(), commit: "1111111".into(), channel: Channel::Release };
        let mut b = AutoCheck::default();
        assert!(!b.found(Ok(release("v0.43")), &installed));
        assert!(b.newer().is_none());
        assert!(b.found(Ok(release("v0.44")), &installed));
        assert_eq!(b.newer().map(|m| m.version.as_str()), Some("v0.44"));
        assert!(b.set(0, None));
        assert!(b.newer().is_none());
    }

    fn settle(b: &mut AutoCheck, dir: &Path, now: u64) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while b.answer.is_some() && std::time::Instant::now() < until {
            b.poll(dir, now, false);
        }
    }

    #[test]
    fn a_failed_check_shows_nothing_and_waits_half_an_hour() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = AutoCheck::default();
        b.start(1_000, || Err("no internet".into()));
        settle(&mut b, dir.path(), 1_000);
        assert!(b.answer.is_none() && b.newer().is_none());
        b.poll(dir.path(), 1_000 + RECHECK_MS - 1, true);
        assert!(b.answer.is_none(), "asked again too soon");
        b.poll(dir.path(), 1_000 + RECHECK_MS, true);
        assert!(b.answer.is_some(), "not asked after half an hour");
    }

    #[test]
    fn closing_a_game_checks_again_straight_away() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = AutoCheck::default();
        b.set(1_000, None);
        b.poll(dir.path(), 2_000, true);
        assert!(b.answer.is_none());
        b.soon();
        b.poll(dir.path(), 2_000, true);
        assert!(b.answer.is_some());
    }

    #[test]
    fn install_takes_a_newer_update_out_since_the_check_instead() {
        let (old, new) = (release("v0.44"), release("v0.45"));
        let mut ran = false;
        assert_eq!(install_latest(&old, Ok(new.clone()), || {
            ran = true;
            Ok(())
        }), Ok(Some(new)));
        assert!(!ran, "installed the old one");
        assert_eq!(install_latest(&old, Ok(old.clone()), || {
            ran = true;
            Ok(())
        }), Ok(None));
        assert!(ran);
        assert!(install_latest(&old, Err("no internet".into()), || panic!("installed unchecked")).is_err());
    }

    #[test]
    fn a_newer_update_found_at_install_is_shown_to_install_next() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = screen(dir.path());
        found(&mut s);
        let (tx, rx) = mpsc::channel();
        s.installing = Some(Installing { rx, progress: Arc::new(Progress::default()), size: 1, shown: 0 });
        tx.send(Ok(Some(release("v0.45")))).unwrap();
        assert!(s.poll());
        assert!(!s.busy());
        assert_eq!(s.lines()[1], "INSTALL V0.45");
        assert_eq!(s.take_verdict().flatten().map(|m| m.version), Some("v0.45".into()));
    }

    #[test]
    fn an_update_already_found_is_shown_without_checking_again() {
        let dir = tempfile::tempdir().unwrap();
        let s = UpdateScreen::open(dir.path(), Some(release("v99.0")));
        assert!(!s.busy(), "checked again");
        assert_eq!(s.lines()[1], "INSTALL V99.0");
        assert_eq!(s.row(), 1);
    }

    #[test]
    fn a_found_update_for_another_channel_is_checked_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut dev = release("v99.0");
        dev.channel = Channel::Dev;
        assert!(UpdateScreen::open(dir.path(), Some(dev)).busy());
    }

    #[test]
    fn the_channel_row_shows_only_after_the_code() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = screen(dir.path());
        assert_eq!(s.lines(), ["CHECK FOR UPDATES"]);
        assert!(!press(&mut s, &CODE));
        assert_eq!(s.lines().last().unwrap(), "CHANNEL                    RELEASE");
    }

    #[test]
    fn b_alone_still_goes_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = screen(dir.path());
        assert!(press(&mut s, &[Btn::Up, Btn::Down, Btn::B]));
        assert!(!press(&mut s, &[Btn::L1, Btn::R1]));
        assert!(press(&mut s, &CODE[..9].iter().chain(&[Btn::B]).copied().collect::<Vec<_>>()));
    }

    #[test]
    fn the_dev_channel_is_chosen_kept_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Config")).unwrap();
        let mut s = screen(dir.path());
        press(&mut s, &CODE);
        press(&mut s, &[Btn::Up, Btn::Right]);
        assert_eq!(s.channel, Channel::Dev);
        assert!(s.busy());
        assert_eq!(update::saved_channel(dir.path()), Channel::Dev);
        let reopened = UpdateScreen::with(dir.path(), String::new(), s.installed.clone(), update::saved_channel(dir.path()));
        assert!(reopened.unlocked);
    }

    #[test]
    fn l1_and_r1_scroll_the_notes_no_further_than_their_end() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = screen(dir.path());
        s.scroll_max.set(4);
        press(&mut s, &[Btn::R1]);
        assert_eq!(s.scroll(), 3);
        press(&mut s, &[Btn::R1, Btn::R1]);
        assert_eq!(s.scroll(), 4);
        press(&mut s, &[Btn::L1]);
        assert_eq!(s.scroll(), 1);
        press(&mut s, &[Btn::L1]);
        assert_eq!(s.scroll(), 0);
    }

    #[test]
    fn an_update_installs_on_a_second_a_and_not_on_a_flat_battery() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = screen(dir.path());
        found(&mut s);
        assert_eq!(s.lines()[1], "INSTALL V0.44");
        assert_eq!(s.about().1, ["- New: this."]);
        s.battery = Some((12, false));
        press(&mut s, &[Btn::A]);
        assert!(s.status.contains("Charge") && !s.busy());
        s.battery = Some((12, true));
        press(&mut s, &[Btn::A]);
        assert!(s.armed && !s.busy());
        press(&mut s, &[Btn::B]);
        assert!(!s.armed && !s.busy());
        press(&mut s, &[Btn::A, Btn::A]);
        assert!(s.busy());
    }
}
