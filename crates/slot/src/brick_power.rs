//! Conservative runtime CPU ceilings. No voltage, PLL or firmware changes.
use std::{fs, io::{self, Write}, path::{Path, PathBuf}, time::{Duration, Instant}};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Game, Menu, Standby, Boost }
// `at` is the ceiling (scaling_max_freq). In games `low` is also held as
// scaling_min_freq: schedutil reads the frame-paced emulator as idle and parks
// at 816 MHz, so a ceiling alone cannot stop audio underruns.
struct Policy { levels: Vec<u32>, at: usize, floor: usize, clean: u32, mode: Mode, base: usize, low: usize, steady: u32 }
impl Policy {
    fn new(levels: Vec<u32>) -> Self {
        let at=levels.len()-1;
        let floor=levels.iter().position(|&v|v>=1_008_000).unwrap_or(at);
        Self {levels,at,floor,clean:0,mode:Mode::Boost,base:floor,low:0,steady:0}
    }
    fn change_mode(&mut self, mode: Mode) -> u32 {
        if self.mode != mode {
            self.clean=0;
            self.at=match mode {
                Mode::Menu=>self.levels.iter().position(|&v|v>=816_000).unwrap_or(self.levels.len()-1),
                Mode::Standby=>0,
                _=>self.levels.len()-1,
            };
            self.low=if mode==Mode::Game {self.base.min(self.at)} else {0};
            self.steady=0;
            self.mode=mode;
        }
        self.levels[self.at]
    }
    // Called per half-second window, never per rendered frame.
    fn sample(&mut self, bad: bool, spare: bool) -> u32 {
        if self.mode!=Mode::Game {return self.levels[self.at]}
        if bad {
            if self.at+1<self.levels.len() {self.floor=self.floor.max(self.at+1);}
            self.at=(self.at+2).min(self.levels.len()-1);self.clean=0;
            self.low=(self.low+1).min(self.at);self.steady=0;
        } else if spare {
            self.clean+=1;
            if self.clean>=20 {self.at=self.at.saturating_sub(1).max(self.floor);self.clean=0;}
        } else {self.clean=0;}
        if !bad {
            // A minute without trouble lets the held minimum step back down.
            self.steady+=1;
            if self.steady>=120 {self.low=self.low.saturating_sub(1).max(self.base);self.steady=0;}
        }
        self.low=self.low.min(self.at);
        self.levels[self.at]
    }
    fn min_khz(&self) -> u32 {self.levels[self.low]}
}
pub struct CpuPower {
    sys: PathBuf, log: PathBuf, original: Vec<(String,String)>, policy: Option<Policy>,
    userspace: bool, written: Option<u32>, written_min: Option<u32>, window: Instant, frames:u32, late:u32,
    busy:u32, underruns:u64, grace:Instant, telemetry_at:Instant,
}
fn read(sys:&Path,name:&str)->io::Result<String> {Ok(fs::read_to_string(sys.join(name))?.trim().into())}
fn write(sys:&Path,name:&str,value:&str)->io::Result<()> {
    let mut f=fs::OpenOptions::new().write(true).truncate(true).open(sys.join(name))?;
    let command=format!("{value}\n");
    if f.write(command.as_bytes())?!=command.len() {return Err(io::Error::new(io::ErrorKind::WriteZero,"short cpufreq write"))}
    Ok(())
}
impl CpuPower {
    pub fn open(root:&Path)->Self {Self::open_in(root,&slot_power::hw("/sys/devices/system/cpu/cpu0/cpufreq"))}
    fn open_in(root:&Path,sys:&Path)->Self {
        let now=Instant::now();
        let mut s=Self {sys:sys.into(),log:root.join("Logs/power.log"),original:vec![],policy:None,userspace:false,written:None,written_min:None,window:now,frames:0,late:0,busy:0,underruns:0,grace:now+Duration::from_secs(3),telemetry_at:now};
        if root.join("DISABLE_POWER_SAVING").exists() {s.log("adaptive clocks disabled");return s}
        let result=(||->io::Result<()> {
            let min=read(sys,"scaling_min_freq")?.parse::<u32>().map_err(io::Error::other)?;
            let max=read(sys,"scaling_max_freq")?.parse::<u32>().map_err(io::Error::other)?;
            let gov=read(sys,"scaling_governor")?;
            let governors=read(sys,"scaling_available_governors").unwrap_or_else(|_|gov.clone());
            let selected=["schedutil","ondemand","conservative","userspace"].into_iter().find(|g|governors.split_whitespace().any(|v|v==*g)).ok_or_else(||io::Error::other("no supported governor"))?;
            let mut levels:Vec<u32>=read(sys,"scaling_available_frequencies")?.split_whitespace().filter_map(|v|v.parse().ok()).filter(|&v|v>=408_000 && v<=max.min(1_800_000)).collect();
            levels.sort_unstable();levels.dedup();
            if levels.is_empty() {return Err(io::Error::other("no verified frequency table"))}
            s.original=vec![("scaling_min_freq".into(),min.to_string()),("scaling_max_freq".into(),max.to_string()),("scaling_governor".into(),gov.clone())];
            if gov=="userspace" {let speed=read(sys,"scaling_setspeed")?;speed.parse::<u32>().map_err(io::Error::other)?;s.original.push(("scaling_setspeed".into(),speed));}
            // Lower min first so a menu/standby ceiling never crosses it.
            write(sys,"scaling_min_freq",&levels[0].to_string())?;
            write(sys,"scaling_governor",selected)?;
            s.userspace=selected=="userspace";
            s.policy=Some(Policy::new(levels));
            s.log(&format!("enabled governor={selected}; original={:?}",s.original));
            Ok(())
        })();
        if let Err(e)=result {s.log(&format!("disabled: {e}"));s.restore();}
        s
    }
    fn log(&self,message:&str) {
        if let Some(dir)=self.log.parent(){let _=fs::create_dir_all(dir);}
        if fs::metadata(&self.log).is_ok_and(|m|m.len()>131072){let _=fs::rename(&self.log,self.log.with_extension("previous.log"));}
        if let Ok(mut f)=fs::OpenOptions::new().create(true).append(true).open(&self.log){let _=writeln!(f,"{:?} {message}",std::time::SystemTime::now());}
    }
    fn apply(&mut self,value:u32) {
        let low=self.policy.as_ref().map_or(0,|p|p.min_khz()).min(value);
        if self.written==Some(value) && (self.userspace || self.written_min==Some(low)){return}
        let max=|s:&Self|write(&s.sys,"scaling_max_freq",&value.to_string());
        let min=|s:&Self|if s.userspace || s.written_min==Some(low) {Ok(())} else {write(&s.sys,"scaling_min_freq",&low.to_string())};
        // Keep min <= max at every step: raise the ceiling before the floor, lower the floor before the ceiling.
        let raising=self.written.is_none_or(|w|value>=w);
        let result=if raising {max(self).and_then(|_|min(self))} else {min(self).and_then(|_|max(self))}
            .and_then(|_|if self.userspace {write(&self.sys,"scaling_setspeed",&value.to_string())}else{Ok(())});
        if let Err(e)=result {self.log(&format!("write failed: {e}; restoring"));self.restore();return}
        let moved=self.written!=Some(value) || (!self.userspace && self.written_min!=Some(low));
        self.written=Some(value);if !self.userspace {self.written_min=Some(low);}
        if moved {self.log(&format!("mode={:?} ceiling_khz={value} floor_khz={low}",self.policy.as_ref().map(|p|p.mode)));}
    }
    pub fn mode(&mut self,mode:Mode,underruns:u64) {
        if self.telemetry_at.elapsed()>=Duration::from_secs(60) {
            self.telemetry_at=Instant::now();
            let battery=slot_power::hw("/sys/class/power_supply/axp2202-battery");let battery=battery.as_path();
            self.log(&format!("sample mode={mode:?} actual_khz={} capacity={} voltage_uv={} charge_counter_raw={} audio_underruns={underruns}",
                read(&self.sys,"scaling_cur_freq").unwrap_or_default(),read(battery,"capacity").unwrap_or_default(),read(battery,"voltage_now").unwrap_or_default(),read(battery,"charge_counter").unwrap_or_default()));
        }
        let Some(p)=&mut self.policy else{return};
        let changed=p.mode!=mode;
        let value=p.change_mode(mode);
        if changed {self.window=Instant::now();self.grace=self.window+Duration::from_secs(3);self.frames=0;self.late=0;self.busy=0;self.underruns=underruns;}
        self.apply(value);
    }
    pub fn sample(&mut self,frame_ready:bool,work:Duration,budget:Duration,underruns:u64) {
        if self.policy.is_none(){return}
        self.frames+=1;
        self.late+=u32::from(!frame_ready || work>budget);
        self.busy+=u32::from(work>budget.mul_f32(0.65));
        if self.window.elapsed()<Duration::from_millis(500){return}
        let audio_bad=underruns>self.underruns;
        let bad=audio_bad || self.late*20>self.frames;
        let spare=self.busy*20<self.frames && self.late==0 && !audio_bad;
        if Instant::now()>=self.grace {
            let value=self.policy.as_mut().unwrap().sample(bad,spare);self.apply(value);
        }
        self.frames=0;self.late=0;self.busy=0;self.underruns=underruns;self.window=Instant::now();
    }
    pub fn restore(&mut self) {
        self.policy=None;
        // Original max first: our min was only lowered; original min is then valid.
        for key in ["scaling_max_freq","scaling_min_freq","scaling_governor","scaling_setspeed"] {
            if let Some((_,v))=self.original.iter().find(|(k,_)|k==key) {
                if let Err(e)=write(&self.sys,key,v){self.log(&format!("restore {key}: {e}"));}
            }
        }
        self.original.clear();self.written=None;self.written_min=None;
    }
}
impl Drop for CpuPower {fn drop(&mut self){self.restore();}}
// Linux CLOCK_BOOTTIME includes suspend; CLOCK_MONOTONIC excludes it.
pub fn suspended_seconds()->Option<f64> {
    #[repr(C)] struct Timespec {sec:std::ffi::c_long,nsec:std::ffi::c_long}
    unsafe extern "C" {fn clock_gettime(clock:std::ffi::c_int,tp:*mut Timespec)->std::ffi::c_int;}
    let mut boot=Timespec{sec:0,nsec:0};let mut mono=Timespec{sec:0,nsec:0};
    unsafe {if clock_gettime(7,&mut boot)!=0 || clock_gettime(1,&mut mono)!=0{return None}}
    Some((boot.sec-mono.sec) as f64+(boot.nsec-mono.nsec) as f64/1e9)
}
#[cfg(test)] mod tests {
    use super::*;
    fn policy()->Policy {Policy::new(vec![408000,816000,1008000,1200000,1416000,1608000,1800000])}
    #[test] fn slow_down_fast_up_and_remember_failed_level() {
        let mut p=policy();p.change_mode(Mode::Game);
        for _ in 0..19 {assert_eq!(p.sample(false,true),1800000)}
        assert_eq!(p.sample(false,true),1608000);
        assert_eq!(p.sample(true,false),1800000);
        for _ in 0..100 {p.sample(false,true);}
        assert_eq!(p.levels[p.at],1800000);
    }
    #[test] fn games_hold_a_minimum_that_rises_on_trouble_and_decays() {
        let mut p=policy();p.change_mode(Mode::Game);
        assert_eq!(p.min_khz(),1008000);
        p.sample(true,false);assert_eq!(p.min_khz(),1200000);
        p.sample(true,false);assert_eq!(p.min_khz(),1416000);
        for _ in 0..119 {p.sample(false,false);} assert_eq!(p.min_khz(),1416000);
        p.sample(false,false);assert_eq!(p.min_khz(),1200000);
        for _ in 0..1000 {p.sample(false,true);} assert_eq!(p.min_khz(),1008000);
        assert!(p.min_khz()<=p.levels[p.at]);
        assert_eq!(p.change_mode(Mode::Menu),816000);assert_eq!(p.min_khz(),408000);
    }
    #[test] fn floor_written_in_games_and_released_in_menu() {
        let (t,sys)=fixture();fs::write(sys.join("scaling_available_governors"),"schedutil").unwrap();
        let mut cpu2=CpuPower::open_in(t.path(),&sys);
        cpu2.mode(Mode::Game,0);assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"1008000");assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"1800000");
        cpu2.mode(Mode::Menu,0);assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"408000");assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"816000");
        cpu2.mode(Mode::Standby,0);assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"408000");
        cpu2.mode(Mode::Game,0);assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"1008000");
        drop(cpu2);assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"1200000");assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"2000000");
    }
    #[test] fn floors_and_transitions_protect_gameplay() {
        let mut p=policy();assert_eq!(p.change_mode(Mode::Standby),408000);
        assert_eq!(p.change_mode(Mode::Menu),816000);
        assert_eq!(p.change_mode(Mode::Game),1800000);
        for _ in 0..400 {p.sample(false,true);}
        assert_eq!(p.levels[p.at],1008000);
        assert_eq!(p.change_mode(Mode::Boost),1800000);
        for _ in 0..100 {assert_eq!(p.sample(false,true),1800000);}
    }
    fn fixture()->(tempfile::TempDir,PathBuf) {
        let t=tempfile::tempdir().unwrap();let sys=t.path().join("cpu");fs::create_dir(&sys).unwrap();
        for (n,v) in [("scaling_governor","userspace"),("scaling_available_governors","userspace schedutil"),("scaling_min_freq","1200000"),("scaling_max_freq","2000000"),("scaling_setspeed","1608000"),("scaling_available_frequencies","2000000 1800000 1608000 1200000 1008000 816000 408000")] {fs::write(sys.join(n),v).unwrap();}
        (t,sys)
    }
    #[test] fn restores_original_policy_and_does_not_overclock() {
        let (t,sys)=fixture();let mut cpu=CpuPower::open_in(t.path(),&sys);
        cpu.mode(Mode::Game,0);assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"1800000");
        cpu.mode(Mode::Standby,0);assert_eq!(read(&sys,"scaling_max_freq").unwrap(),"408000");
        drop(cpu);
        for (n,v) in [("scaling_min_freq","1200000"),("scaling_max_freq","2000000"),("scaling_governor","userspace"),("scaling_setspeed","1608000")] {assert_eq!(read(&sys,n).unwrap(),v);}
    }
    #[test] fn missing_table_and_optout_leave_hardware_unchanged() {
        let (t,sys)=fixture();fs::remove_file(sys.join("scaling_available_frequencies")).unwrap();
        let cpu=CpuPower::open_in(t.path(),&sys);assert!(cpu.policy.is_none());assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"1200000");
        fs::write(t.path().join("DISABLE_POWER_SAVING"),"").unwrap();let cpu=CpuPower::open_in(t.path(),&sys);assert!(cpu.policy.is_none());
    }
    #[test] fn userspace_only_firmware_gets_explicit_speed() {
        let (t,sys)=fixture();fs::write(sys.join("scaling_available_governors"),"userspace").unwrap();
        let mut cpu=CpuPower::open_in(t.path(),&sys);cpu.mode(Mode::Menu,0);
        assert_eq!(read(&sys,"scaling_setspeed").unwrap(),"816000");cpu.restore();assert_eq!(read(&sys,"scaling_setspeed").unwrap(),"1608000");
    }
    #[test] fn failed_write_disables_controller_and_rolls_back() {
        let (t,sys)=fixture();let mut cpu=CpuPower::open_in(t.path(),&sys);
        fs::remove_file(sys.join("scaling_max_freq")).unwrap();fs::create_dir(sys.join("scaling_max_freq")).unwrap();
        cpu.mode(Mode::Standby,0);assert!(cpu.policy.is_none());assert_eq!(read(&sys,"scaling_min_freq").unwrap(),"1200000");assert_eq!(read(&sys,"scaling_governor").unwrap(),"userspace");
    }
}
