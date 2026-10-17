//! Brick LED driver integration, following NextUI's tg5040 sysfs mapping.
use std::{fs, io::{self, Write}, path::{Path, PathBuf}};
use slot_power::LedState;
use slot_ui::{QuickRow as Row, QuickValue as Value};
const FILE: &str = "Config/brick_leds.ini";
const COLORS: [&str; 6] = ["FFFFFF", "0000FF", "00FF00", "FF8000", "FF0000", "8000FF"];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Settings { mode: u8, top: u8, buttons: u8, triggers: u8, color: u8, effect: u8 }
impl Default for Settings {
    fn default() -> Self { Self { mode: 2, top: 1, buttons: 1, triggers: 1, color: 0, effect: 0 } }
}
impl Settings {
    fn read(text: &str) -> Self {
        let mut s=Self::default();
        for line in text.lines() {
            let Some((key,value))=line.split_once('=') else { continue };
            let Ok(n)=value.trim().parse::<u8>() else { continue };
            match key.trim() {
                "mode" if n<=2 => s.mode=if n==0 {2} else {n}, "top" if n<=3 => s.top=n,
                "buttons" if n<=3 => s.buttons=n, "triggers" if n<=3 => s.triggers=n,
                "color" if n<6 => s.color=n, "effect" if n<2 => s.effect=n, _=>{}
            }
        }
        s
    }
    fn text(self) -> String { format!("mode={}\ntop={}\nbuttons={}\ntriggers={}\ncolor={}\neffect={}\n",self.mode,self.top,self.buttons,self.triggers,self.color,self.effect) }
    fn step(&mut self, row: Row, right: bool) {
        if row==Row::LedMode { self.mode=if self.mode==1 {2} else {1}; return; }
        let (v,n)=match row {
            Row::LedMode => (&mut self.mode,3), Row::LedTop => (&mut self.top,4),
            Row::LedButtons => (&mut self.buttons,4), Row::LedTriggers => (&mut self.triggers,4),
            Row::LedColor => (&mut self.color,6), Row::LedEffect => (&mut self.effect,2), _=>return
        };
        *v=(*v+if right {1} else {n-1})%n;
        if row!=Row::LedMode { self.mode=2; }
    }
}
// Each sysfs write invokes the driver. Send value and terminator together;
// formatting into File directly can send a second, empty command.
fn write_command(out: &mut impl Write, name: &str, value: &str) -> io::Result<()> {
    let separator=if name.starts_with("effect_rgb_hex_") {" "} else {""};
    let command=format!("{}{separator}\n",value.trim());
    if out.write(command.as_bytes())? != command.len() {
        return Err(io::Error::new(io::ErrorKind::WriteZero,"short LED sysfs command"));
    }
    Ok(())
}
pub struct BrickLeds { root: PathBuf, sys: PathBuf, settings: Settings, original: Vec<(String,String)>, sleeping: Option<Vec<(String,String)>>, previous_mode: Option<u8>, power_state: LedState, available: bool, #[cfg(test)] applied_brightness: std::cell::RefCell<Vec<(String,String)>> }
impl BrickLeds {
    pub fn open(root: &Path) -> Self { Self::open_in(root,&slot_power::hw("/sys/class/led_anim")) }
    fn names() -> Vec<String> {
        let mut names=vec!["max_scale".into(),"max_scale_f1f2".into(),"max_scale_lr".into()];
        for zone in ["m","f1","f2","lr"] {
            for prefix in ["effect_rgb_hex", "effect_cycles", "effect_duration", "effect"] { names.push(format!("{prefix}_{zone}")); }
        }
        names.push("effect_enable".into()); names
    }
    fn open_in(root: &Path, sys: &Path) -> Self {
        let original: Vec<_>=Self::names().into_iter().filter_map(|name| fs::read_to_string(sys.join(&name)).ok().map(|v|(name,v.trim().to_owned()))).collect();
        let available=original.len()==Self::names().len();
        let settings=Settings::read(&fs::read_to_string(root.join(FILE)).unwrap_or_default());
        let mut this=Self {root:root.into(),sys:sys.into(),settings,original,sleeping:None,previous_mode:None,power_state:LedState::Running,available, #[cfg(test)] applied_brightness: Default::default()};
        if available && settings.mode!=0 {
            if let Err(e)=this.apply(settings) { eprintln!("slot: LED startup: {e}"); let _=this.restore(); this.available=false; }
        }
        this.log_state("startup");
        this
    }
    fn write(&self, name: &str, value: &str) -> io::Result<()> {
        // Sysfs attributes must exist; never create a replacement regular file.
        let mut f=fs::OpenOptions::new().write(true).truncate(true).open(self.sys.join(name))?;
        write_command(&mut f, name, value)?;
        // Model the driver latching brightness when an effect is triggered.
        #[cfg(test)]
        if let Some(scale)=match name { "effect_m"=>Some("max_scale"), "effect_f1"|"effect_f2"=>Some("max_scale_f1f2"), "effect_lr"=>Some("max_scale_lr"), _=>None } {
            self.applied_brightness.borrow_mut().push((name.into(),fs::read_to_string(self.sys.join(scale))?.trim().into()));
        }
        Ok(())
    }
    pub fn set_power_state(&mut self, state: LedState) -> io::Result<()> {
        self.power_state = state;
        if self.available && self.sleeping.is_none() {
            self.apply(self.settings)?;
        }
        Ok(())
    }
    pub fn restore(&self) -> io::Result<()> {
        if self.settings.mode==0 { return Ok(()) }
        self.restore_snapshot()
    }
    pub fn sleep(&mut self) -> io::Result<()> {
        if !self.available || self.sleeping.is_some() { return Ok(()) }
        let snapshot = Self::names().into_iter().map(|name| {
            fs::read_to_string(self.sys.join(&name)).map(|v|(name,v.trim().to_owned()))
        }).collect::<io::Result<Vec<_>>>()?;
        self.sleeping = Some(snapshot);
        self.apply(Settings { mode: 1, ..self.settings })?;
        self.write("effect_enable", "0")?;
        self.log_state("sleep");
        Ok(())
    }
    pub fn wake(&mut self) -> io::Result<()> {
        if let Some(snapshot) = &self.sleeping {
            self.restore_values(snapshot)?;
            self.sleeping = None;
            self.apply(self.settings)?;
            self.log_state("wake");
        }
        Ok(())
    }
    fn restore_snapshot(&self) -> io::Result<()> {
        self.restore_values(&self.original)
    }
    fn restore_values(&self, values: &[(String,String)]) -> io::Result<()> {
        if !self.available { return Ok(()) }
        // effect_<zone> is the apply trigger: restore all parameters first.
        let is_trigger=|name: &str| matches!(name,"effect_m"|"effect_f1"|"effect_f2"|"effect_lr");
        for (name,value) in values.iter().filter(|(n,_)| !is_trigger(n)) { self.write(name,value)?; }
        for (name,value) in values.iter().filter(|(n,_)| is_trigger(n)) { self.write(name,value)?; }
        Ok(())
    }
    fn apply(&self,s: Settings) -> io::Result<()> {
        if s.mode==0 { return self.restore_snapshot() }
        let warning = matches!(self.power_state, LedState::Low | LedState::VeryLow) && self.sleeping.is_none();
        let off = s.mode == 1 || self.power_state == LedState::Off;
        self.write("effect_enable","1")?;
        for (name,level) in [("max_scale",s.top),("max_scale_f1f2",s.buttons),("max_scale_lr",s.triggers)] {
            let brightness = if warning && name == "max_scale_f1f2" { 20 }
                else if off { 0 } else { u32::from(level) * 20 };
            self.write(name,&brightness.to_string())?;
        }
        for zone in ["m","f1","f2","lr"] {
            let alarm = warning && matches!(zone, "f1" | "f2");
            let blink = alarm && self.power_state == LedState::VeryLow;
            self.write(&format!("effect_rgb_hex_{zone}"),if alarm { "FF0000" } else { COLORS[s.color as usize] })?;
            self.write(&format!("effect_cycles_{zone}"),"-1")?;
            self.write(&format!("effect_duration_{zone}"),if blink { "500" } else { "2000" })?;
            // NextUI LEDS_updateLeds uses this order: the effect write applies
            // brightness/color/timing, including zero brightness for Off.
            self.write(&format!("effect_{zone}"),if blink { "5" } else if alarm || s.effect==0 {"4"} else {"2"})?;
        }
        Ok(())
    }
    fn log_state(&self, action: &str) {
        let dir=self.root.join("Logs");let path=dir.join("leds.log");
        let _=fs::create_dir_all(&dir);
        if fs::metadata(&path).map(|m|m.len()>131072).unwrap_or(false) {
            let _=fs::rename(&path,dir.join("leds.previous.log"));
        }
        if let Ok(mut log)=fs::OpenOptions::new().create(true).append(true).open(path) {
            let _=writeln!(log,"v0.7 {action}: {:?}",self.settings);
            for name in Self::names().into_iter().chain(["effect_names".into()]) {
                let value=fs::read_to_string(self.sys.join(&name)).unwrap_or_else(|e|format!("error: {e}"));
                let _=writeln!(log,"{name}={}",value.trim());
            }
        }
    }
    pub fn value(&self,row: Row) -> Option<Value> {
        if !self.available { return Some(Value::Unavailable) }
        let s=self.settings;
        let brightness=|v:u8| [Value::Off,Value::Low,Value::Medium,Value::High][v as usize];
        Some(match row {
            Row::LedMode => if s.mode==1 {Value::Off} else {Value::On},
            Row::LedTop => brightness(s.top), Row::LedButtons => brightness(s.buttons), Row::LedTriggers => brightness(s.triggers),
            Row::LedColor => [Value::White,Value::Blue,Value::Green,Value::Amber,Value::Red,Value::Purple][s.color as usize],
            Row::LedEffect => [Value::Steady,Value::Breathe][s.effect as usize], _=>return None
        })
    }
    pub fn toggle(&mut self) -> io::Result<()> {
        let before = self.settings.mode;
        let mut next = self.settings;
        next.mode = if before == 1 { self.previous_mode.unwrap_or(2) } else { 1 };
        self.commit(next)?;
        if before != 1 { self.previous_mode = Some(before); }
        Ok(())
    }
    pub fn change(&mut self,row: Row,right: bool) -> io::Result<()> {
        let mut next=self.settings;next.step(row,right);
        self.commit(next)
    }
    fn commit(&mut self, next: Settings) -> io::Result<()> {
        if !self.available { return Err(io::Error::new(io::ErrorKind::NotFound,"Brick LED controls unavailable")) }
        if let Err(e)=self.apply(next) { let _=self.apply(self.settings);return Err(e) }
        let path=self.root.join(FILE); let tmp=path.with_extension("tmp");
        let saved=(|| {fs::create_dir_all(path.parent().unwrap())?;let mut f=fs::File::create(&tmp)?;f.write_all(next.text().as_bytes())?;f.sync_all()?;fs::rename(&tmp,&path)})();
        if let Err(e)=saved { let _=self.apply(self.settings); return Err(e) }
        self.settings=next; self.log_state("change"); Ok(())
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn battery_leds() -> (tempfile::TempDir, PathBuf, BrickLeds) {
        let t = tempfile::tempdir().unwrap();
        let sys = t.path().join("sys");
        fs::create_dir(&sys).unwrap();
        for name in BrickLeds::names() { fs::write(sys.join(name), "7\n").unwrap(); }
        let leds = BrickLeds::open_in(t.path(), &sys);
        (t, sys, leds)
    }
    fn read(sys: &Path, name: &str) -> String {
        fs::read_to_string(sys.join(name)).unwrap().trim().to_owned()
    }
    #[test]
    fn warning_changes_between_solid_and_fast_blink_with_lights_off() {
        let (_t, sys, mut leds) = battery_leds();
        leds.toggle().unwrap();
        for (state, effect, duration) in [
            (LedState::Low, "4", "2000"),
            (LedState::VeryLow, "5", "500"),
            (LedState::Low, "4", "2000"),
        ] {
            leds.set_power_state(state).unwrap();
            for zone in ["f1", "f2"] {
                assert_eq!(read(&sys, &format!("effect_rgb_hex_{zone}")), "FF0000");
                assert_eq!(read(&sys, &format!("effect_{zone}")), effect);
                assert_eq!(read(&sys, &format!("effect_duration_{zone}")), duration);
            }
            assert_eq!(read(&sys, "max_scale_f1f2"), "20");
            assert_eq!(read(&sys, "max_scale"), "0");
            assert_eq!(read(&sys, "max_scale_lr"), "0");
            leds.sleep().unwrap();
            assert_eq!(read(&sys, "effect_enable"), "0");
            leds.wake().unwrap();
            assert_eq!(read(&sys, "effect_f1"), effect);
        }
        leds.set_power_state(LedState::Charging).unwrap();
        assert_eq!(read(&sys, "max_scale_f1f2"), "0");
        assert_eq!(read(&sys, "effect_rgb_hex_f1"), "FFFFFF");
    }
    #[test]
    fn low_battery_overrides_only_front_lights_even_when_disabled() {
        let (t, sys, mut leds) = battery_leds();
        leds.change(Row::LedColor, true).unwrap();
        leds.change(Row::LedEffect, true).unwrap();
        let config = fs::read(t.path().join(FILE)).unwrap();
        leds.set_power_state(LedState::VeryLow).unwrap();
        for zone in ["f1", "f2"] {
            assert_eq!(read(&sys, &format!("effect_rgb_hex_{zone}")), "FF0000");
            assert_eq!(read(&sys, &format!("effect_{zone}")), "5");
            assert_eq!(read(&sys, &format!("effect_duration_{zone}")), "500");
            assert_eq!(read(&sys, &format!("effect_cycles_{zone}")), "-1");
        }
        assert_eq!(read(&sys, "max_scale_f1f2"), "20");
        assert_eq!(read(&sys, "effect_rgb_hex_m"), "0000FF");
        assert_eq!(read(&sys, "effect_lr"), "2");
        assert_eq!(fs::read(t.path().join(FILE)).unwrap(), config);
        leds.toggle().unwrap();
        assert_eq!(read(&sys, "max_scale"), "0");
        assert_eq!(read(&sys, "max_scale_lr"), "0");
        assert_eq!(read(&sys, "max_scale_f1f2"), "20");
        assert_eq!(read(&sys, "effect_f1"), "5");
        leds.set_power_state(LedState::Charging).unwrap();
        assert_eq!(read(&sys, "max_scale_f1f2"), "0");
        assert_eq!(read(&sys, "effect_rgb_hex_f1"), "0000FF");
        assert_eq!(read(&sys, "effect_f1"), "2");
    }
    #[test]
    fn clearing_warning_restores_settings_changed_during_warning() {
        let (_t, sys, mut leds) = battery_leds();
        leds.set_power_state(LedState::Low).unwrap();
        leds.change(Row::LedColor, true).unwrap();
        leds.change(Row::LedButtons, true).unwrap();
        leds.change(Row::LedEffect, true).unwrap();
        assert_eq!(read(&sys, "effect_rgb_hex_f2"), "FF0000");
        assert_eq!(read(&sys, "max_scale_f1f2"), "20");
        for state in [LedState::Running, LedState::Charging, LedState::Charged] {
            leds.set_power_state(LedState::Low).unwrap();
            leds.set_power_state(state).unwrap();
            assert_eq!(read(&sys, "effect_rgb_hex_f2"), "0000FF");
            assert_eq!(read(&sys, "effect_f2"), "2");
            assert_eq!(read(&sys, "max_scale_f1f2"), "40");
        }
    }
    #[test]
    fn battery_warning_stays_dark_during_sleep_and_shutdown() {
        let (_t, sys, mut leds) = battery_leds();
        leds.set_power_state(LedState::VeryLow).unwrap();
        leds.sleep().unwrap();
        assert_eq!(read(&sys, "max_scale_f1f2"), "0");
        assert_eq!(read(&sys, "effect_enable"), "0");
        leds.wake().unwrap();
        assert_eq!(read(&sys, "effect_f1"), "5");
        assert_eq!(read(&sys, "max_scale_f1f2"), "20");
        leds.sleep().unwrap();
        leds.set_power_state(LedState::Charging).unwrap();
        assert_eq!(read(&sys, "effect_enable"), "0");
        assert_eq!(read(&sys, "max_scale_f1f2"), "0");
        leds.wake().unwrap();
        assert_eq!(read(&sys, "effect_f1"), "4");
        assert_eq!(read(&sys, "effect_rgb_hex_f1"), "FFFFFF");
        leds.set_power_state(LedState::VeryLow).unwrap();
        leds.set_power_state(LedState::Off).unwrap();
        for name in ["max_scale", "max_scale_f1f2", "max_scale_lr"] {
            assert_eq!(read(&sys, name), "0");
        }
    }
    #[test] fn shortcut_toggle_preserves_settings_and_sleep_stays_off() {
        for mode in [0,2] {
            let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");fs::create_dir(&sys).unwrap();
            for name in BrickLeds::names() {fs::write(sys.join(name),"7\n").unwrap();}
            let mut leds=BrickLeds::open_in(t.path(),&sys);
            if mode==2 {leds.change(Row::LedColor,true).unwrap();leds.change(Row::LedEffect,true).unwrap();}
            let before=leds.settings;
            leds.toggle().unwrap();assert_eq!(leds.settings.mode,1);
            assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap().trim(),"0");
            leds.sleep().unwrap();leds.wake().unwrap();
            assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap().trim(),"0");
            leds.toggle().unwrap();assert_eq!(leds.settings,before);
            assert_eq!(Settings::read(&fs::read_to_string(t.path().join(FILE)).unwrap()),before);
            leds.toggle().unwrap();leds.toggle().unwrap();assert_eq!(leds.settings,before);
        }
    }
    #[test] fn sleep_restores_stock_off_and_custom_without_saving() {
        for mode in 0..3 {
            let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");fs::create_dir(&sys).unwrap();
            for name in BrickLeds::names() {fs::write(sys.join(name),"7\n").unwrap();}
            let mut leds=BrickLeds::open_in(t.path(),&sys);
            for _ in 0..mode {leds.change(Row::LedMode,true).unwrap();}
            let read=|| BrickLeds::names().into_iter().map(|n|fs::read_to_string(sys.join(n)).unwrap().trim().to_owned()).collect::<Vec<_>>();
            let before=read();let config=fs::read(t.path().join(FILE)).ok();
            leds.sleep().unwrap();leds.sleep().unwrap();
            for n in ["max_scale","max_scale_f1f2","max_scale_lr","effect_enable"] {assert_eq!(fs::read_to_string(sys.join(n)).unwrap().trim(),"0");}
            assert_eq!(fs::read(t.path().join(FILE)).ok(),config);
            leds.wake().unwrap();assert_eq!(read(),before);
            leds.wake().unwrap();assert_eq!(read(),before);
            leds.sleep().unwrap();leds.wake().unwrap();assert_eq!(read(),before);
        }
    }
    #[test] fn sysfs_mapping_persistence_and_restore() {
        let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");fs::create_dir(&sys).unwrap();
        for name in BrickLeds::names() {fs::write(sys.join(name),"7\n").unwrap();}
        let mut leds=BrickLeds::open_in(t.path(),&sys);
        assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap(),"20\n");
        leds.change(Row::LedMode,true).unwrap(); // Stock -> Off
        for name in ["max_scale","max_scale_f1f2","max_scale_lr"] {assert_eq!(fs::read_to_string(sys.join(name)).unwrap(),"0\n");}
        leds.change(Row::LedTop,true).unwrap(); // activates On, medium top brightness
        assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap(),"40\n");
        assert_eq!(fs::read_to_string(sys.join("max_scale_f1f2")).unwrap(),"20\n");
        assert_eq!(fs::read_to_string(sys.join("effect_m")).unwrap(),"4\n");
        leds.change(Row::LedEffect,true).unwrap();assert_eq!(fs::read_to_string(sys.join("effect_f2")).unwrap(),"2\n");
        leds.change(Row::LedColor,true).unwrap();assert_eq!(fs::read_to_string(sys.join("effect_rgb_hex_lr")).unwrap(),"0000FF \n");
        assert_eq!(Settings::read(&fs::read_to_string(t.path().join(FILE)).unwrap()),leds.settings);
        leds.restore().unwrap();assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap(),"7\n");
        leds.change(Row::LedMode,true).unwrap(); // On -> Stock
        assert_eq!(leds.value(Row::LedMode),Some(Value::Off));
        assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap(),"0\n");
        leds.change(Row::LedMode,false).unwrap(); // Stock -> On
        let again=BrickLeds::open_in(t.path(),&sys);assert_eq!(again.value(Row::LedColor),Some(Value::Blue));
        assert_eq!(fs::read_to_string(sys.join("max_scale")).unwrap(),"40\n");
    }
    #[test] fn effect_triggers_latch_brightness_on_off_and_restore() {
        let t=tempfile::tempdir().unwrap();let sys=t.path().join("sys");fs::create_dir(&sys).unwrap();
        for name in BrickLeds::names() {fs::write(sys.join(name),"7\n").unwrap();}
        let mut leds=BrickLeds::open_in(t.path(),&sys);
        let check=|leds: &BrickLeds, expected: [&str;4]| {
            let values=leds.applied_brightness.replace(Vec::new());
            assert_eq!(values, ["effect_m","effect_f1","effect_f2","effect_lr"].into_iter().zip(expected).map(|(n,v)|(n.to_owned(),v.to_owned())).collect::<Vec<_>>());
        };
        leds.applied_brightness.borrow_mut().clear();
        leds.change(Row::LedMode,true).unwrap();check(&leds,["0";4]); // On -> Off
        leds.change(Row::LedMode,true).unwrap();check(&leds,["20";4]); // Off -> On
        leds.change(Row::LedMode,false).unwrap();check(&leds,["0";4]); // On -> Off
        leds.change(Row::LedMode,true).unwrap();check(&leds,["20";4]); // repeat
        leds.change(Row::LedTop,true).unwrap();check(&leds,["40","20","20","20"]);
        leds.change(Row::LedEffect,true).unwrap();check(&leds,["40","20","20","20"]);
        leds.restore().unwrap();check(&leds,["7";4]);
        leds.change(Row::LedMode,true).unwrap();check(&leds,["0";4]); // On -> Off
    }
    #[test] fn sysfs_commands_are_single_complete_writes() {
        #[derive(Default)] struct Driver {commands: Vec<Vec<u8>>, value: u32}
        impl Write for Driver {
            fn write(&mut self,b: &[u8])->io::Result<usize> {
                self.commands.push(b.to_vec());
                self.value=String::from_utf8_lossy(b).trim().parse().unwrap_or(0);
                Ok(b.len())
            }
            fn flush(&mut self)->io::Result<()> {Ok(())}
        }
        let mut driver=Driver::default();
        write_command(&mut driver,"max_scale","20").unwrap();
        assert_eq!(driver.value,20);
        assert_eq!(driver.commands,vec![b"20\n".to_vec()]);
        driver.commands.clear();
        write_command(&mut driver,"effect_rgb_hex_m","FFFFFF").unwrap();
        assert_eq!(driver.commands,vec![b"FFFFFF \n".to_vec()]);
        struct Short;
        impl Write for Short {
            fn write(&mut self,_:&[u8])->io::Result<usize>{Ok(1)}
            fn flush(&mut self)->io::Result<()>{Ok(())}
        }
        assert!(write_command(&mut Short,"max_scale","20").is_err());
    }
    #[test] fn legacy_stock_migrates_to_on_and_mode_never_cycles_to_stock() {
        let mut s=Settings::read("mode=0"); assert_eq!(s.mode,2);
        for right in [true,false,true,false] { s.step(Row::LedMode,right); assert_eq!(s.mode,1); s.step(Row::LedMode,right); assert_eq!(s.mode,2); }
        assert_eq!(Settings::read("mode=1").mode,1);
    }
    #[test] fn missing_hardware_and_invalid_settings_are_safe() {
        let t=tempfile::tempdir().unwrap();let mut leds=BrickLeds::open_in(t.path(),&t.path().join("missing"));
        assert_eq!(leds.value(Row::LedMode),Some(Value::Unavailable));assert!(leds.change(Row::LedMode,true).is_err());
        assert!(!t.path().join("missing").exists());
        assert_eq!(Settings::read("mode=99\ncolor=255\ntop=-1\neffect=oops"),Settings::default());
    }
}
