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
    rx: mpsc::Receiver<Result<(), String>>,
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
}

impl UpdateScreen {
    pub fn open(root: &Path) -> Self {
        let channel = update::saved_channel(root);
        let mut s = Self::with(root, update::base(), Installed::current(), channel);
        s.start_check();
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
                    let offer = update::offer(&self.installed, &m);
                    self.status = self.found_line(&m, offer);
                    self.found = Some((m, offer));
                    if offer != Offer::UpToDate {
                        self.row = 1;
                    }
                }
                Err(e) => self.status = e,
            }
            return true;
        }
        let Some(job) = &mut self.installing else { return false };
        match job.rx.try_recv() {
            Ok(Ok(())) => {
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
            Btn::B => return true,
            Btn::Up => {
                self.row = (self.row + rows.len() - 1) % rows.len();
                self.armed = false;
            }
            Btn::Down => {
                self.row = (self.row + 1) % rows.len();
                self.armed = false;
            }
            Btn::Left | Btn::Right if row == Row::Channel => self.switch_channel(),
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
            let _ = tx.send(update::install(&base, &m, &card, &shared));
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
