//! Brick Wi-Fi settings. Uses the firmware supplicant; no shell interpolation of credentials.
use std::{fs,os::unix::{net::UnixDatagram,fs::PermissionsExt},path::{Path,PathBuf},process::{Command,Stdio},sync::{mpsc,Arc,atomic::{AtomicBool,AtomicU64,Ordering}},thread,time::{Duration,Instant}};
use slot_input::{Action,Btn};
use slot_store::Platform;
use slot_ui::{TexId,UndoFace};
use crate::cart_art::{ArtJob,Fetched,Progress};
use crate::ra;
type Result<T> = std::result::Result<T,String>;
static SERIAL:AtomicU64=AtomicU64::new(0);
#[derive(Clone,Copy,PartialEq,Eq,Debug)]
enum Security {Open,Personal,Unsupported}
#[derive(Clone,Debug)]
struct Network {ssid:Vec<u8>,signal:i32,security:Security,saved:Option<u32>}
fn decode(s:&str)->Vec<u8> {
    let b=s.as_bytes();let mut v=Vec::new();let mut i=0;
    while i<b.len(){
        if b[i]==b'\\' && i+1<b.len(){
            if b[i+1]==b'x' && i+3<b.len(){if let Some(n)=std::str::from_utf8(&b[i+2..i+4]).ok().and_then(|s|u8::from_str_radix(s,16).ok()){v.push(n);i+=4;continue}}
            let c=match b[i+1]{b'\\'=>Some(b'\\'),b'"'=>Some(b'"'),b'n'=>Some(b'\n'),b't'=>Some(b'\t'),b'r'=>Some(b'\r'),_=>None};
            if let Some(c)=c {v.push(c);i+=2;continue}
        }v.push(b[i]);i+=1;
    }v
}
fn name(b:&[u8])->String{String::from_utf8_lossy(b).chars().map(|c|if c.is_control(){' '}else{c}).collect()}
fn hex(b:&[u8])->String{b.iter().map(|b|format!("{b:02x}")).collect()}
fn psk(s:&str)->Result<String>{
    if s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit()){return Ok(s.into())}
    if !(8..=63).contains(&s.len()) || !s.bytes().all(|b|(32..=126).contains(&b)){return Err("Use 8-63 characters or 64 hex digits".into())}
    // wpa_config_parse_psk takes the literal bytes between the first and last quote.
    // Backslashes and interior quotes must NOT be C-escaped here.
    Ok(format!("\"{s}\""))
}
fn scan_rows(scan:&str,saved:&str)->Vec<Network>{
    let saved:Vec<_>=saved.lines().skip(1).filter_map(|s|{let v:Vec<_>=s.split('\t').collect();Some((v.first()?.parse::<u32>().ok()?,decode(v.get(1)?)))}).collect();
    let mut out:Vec<Network>=Vec::new();
    for s in scan.lines().skip(1){let v:Vec<_>=s.splitn(5,'\t').collect();if v.len()!=5{continue}let ssid=decode(v[4]);if ssid.is_empty() || ssid.len()>32{continue}
        let security=if v[3].contains("PSK"){Security::Personal}else if ["WPA","WEP","EAP","SAE","OWE","RSN"].iter().any(|s|v[3].contains(s)){Security::Unsupported}else{Security::Open};
        let n=Network{saved:saved.iter().find(|(_,s)|s==&ssid).map(|(id,_)|*id),ssid,security,signal:v[2].parse().unwrap_or(-100)};
        if let Some(old)=out.iter_mut().find(|a|a.ssid==n.ssid && a.security==n.security){if old.signal<n.signal{*old=n}}else{out.push(n)}
    }out.sort_by(|a,b|b.signal.cmp(&a.signal).then(a.ssid.cmp(&b.ssid)));out
}
pub(crate) trait Supplicant {
    fn command(&self,s:&str)->Result<String>;
    fn ok(&self,s:&str)->Result<()>{if self.command(s)?=="OK"{Ok(())}else{Err("Wi-Fi service rejected the request".into())}}
}
pub(crate) struct Control{socket:UnixDatagram,local:PathBuf}
impl Drop for Control{fn drop(&mut self){let _=fs::remove_file(&self.local);}}
impl Control{
    pub(crate) fn at(remote:&Path)->Result<Self>{
        let local=PathBuf::from(format!("/tmp/slot-wifi-{}-{}",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        let socket=UnixDatagram::bind(&local).map_err(|_|"Cannot open Wi-Fi control socket")?;
        let c=Self{socket,local};fs::set_permissions(&c.local,fs::Permissions::from_mode(0o600)).map_err(|_|"Cannot secure Wi-Fi socket")?;
        c.socket.set_read_timeout(Some(Duration::from_secs(2))).map_err(|_|"Cannot set Wi-Fi timeout")?;
        c.socket.set_write_timeout(Some(Duration::from_secs(2))).map_err(|_|"Cannot set Wi-Fi timeout")?;
        c.socket.connect(remote).map_err(|_|"Wi-Fi service unavailable")?;Ok(c)
    }
    fn open()->Result<Self>{SOCKETS.iter().find_map(|s|Self::at(Path::new(s)).ok()).ok_or("Wi-Fi service unavailable".into())}
}
impl Supplicant for Control{
    fn command(&self,s:&str)->Result<String>{
        self.socket.send(s.as_bytes()).map_err(|_|"Wi-Fi command failed")?;let mut b=[0;32768];let n=self.socket.recv(&mut b).map_err(|_|"Wi-Fi service did not respond")?;
        let s=String::from_utf8_lossy(&b[..n]).trim_end_matches('\n').to_string();
        if s.starts_with("FAIL") || s.starts_with("UNKNOWN"){Err("Wi-Fi service rejected the request".into())}else{Ok(s)}
    }
}
fn field<'a>(s:&'a str,key:&str)->Option<&'a str>{s.lines().find_map(|s|s.strip_prefix(key).and_then(|s|s.strip_prefix('=')))}
fn pause(cancel:&AtomicBool,ms:u64)->Result<()>{for _ in 0..(ms+99)/100{if cancel.load(Ordering::Relaxed){return Err("Cancelled".into())}thread::sleep(Duration::from_millis(100));}Ok(())}
/// Slot's own System/brick_wifi.sh always wins: it writes Logs/wifi.log and tries the
/// firmware's /etc/wifi/wifi_init.sh itself, falling back to its own start when that fails.
/// The firmware helper is only run directly when the card copy is missing.
fn helper(firmware:&Path,root:&Path)->Option<PathBuf>{
    let own=root.join("System/brick_wifi.sh");if own.is_file(){return Some(own)}
    firmware.is_file().then(||firmware.into())
}
fn card_root()->PathBuf{PathBuf::from(std::env::var_os("SLOT_ROOT").unwrap_or_else(||"/mnt/SDCARD/Slot".into()))}
fn service(on:bool,cancel:&AtomicBool)->Result<()>{helper_run(if on{"start"}else{"stop"},cancel)}
/// Control sockets slot can reach a supplicant on, in its search order.
const SOCKETS:[&str;3]=["/etc/wifi/sockets/wlan0","/var/run/wpa_supplicant/wlan0","/run/wpa_supplicant/wlan0"];
/// A supplicant is running: what the menu reports as Wi-Fi on, known without asking it.
fn supplicant_up()->bool{SOCKETS.iter().any(|s|Path::new(s).exists())}
/// One helper at a time: the boot repair and the menu may both ask, and two helpers
/// stopping and starting the same supplicant would undo each other.
static HELPER:std::sync::Mutex<()>=std::sync::Mutex::new(());
/// At startup: the firmware starts a supplicant with the radio still blocked, so saved
/// networks are never joined until something repairs it. Do that in the background, as
/// opening the Wi-Fi menu would, so Wi-Fi comes up on its own. The firmware may start its
/// supplicant after slot, so wait for it rather than look once.
pub fn repair_at_boot(){
    let spawned=thread::Builder::new().name("slot-wifi-boot".into()).spawn(||{
        boot_repair(supplicant_up,online,||helper_run("ensure",&AtomicBool::new(false)),|ms|thread::sleep(Duration::from_millis(ms)));
    });
    if let Err(e)=spawned{eprintln!("wifi: boot repair: {e}");}
}
/// How long startup waits for the firmware's supplicant, and for a saved network to join.
const BOOT_WAIT_MS:u64=60_000;
const JOIN_WAIT_MS:u64=20_000;
const BOOT_POLL_MS:u64=1_000;
/// Wait for a supplicant, repair it, and if no network has joined a while later, repair once
/// more: the firmware can still be bringing the radio up the first time.
fn boot_repair(up:impl Fn()->bool,online:impl Fn()->bool,mut ensure:impl FnMut()->Result<()>,sleep:impl Fn(u64)){
    let mut waited=0;
    while !up(){
        if waited>=BOOT_WAIT_MS{eprintln!("wifi: boot repair: no supplicant after {} s; Wi-Fi left off",BOOT_WAIT_MS/1000);return}
        sleep(BOOT_POLL_MS);waited+=BOOT_POLL_MS;
    }
    for attempt in 1..=2{
        if let Err(e)=ensure(){eprintln!("wifi: boot repair {attempt}: {e}");}
        let mut waited=0;
        while waited<JOIN_WAIT_MS{
            if online(){eprintln!("wifi: boot repair: online");return}
            sleep(BOOT_POLL_MS);waited+=BOOT_POLL_MS;
        }
        if !up(){return}
    }
    eprintln!("wifi: boot repair: not online (no saved network in range?)");
}
fn helper_run(verb:&str,cancel:&AtomicBool)->Result<()>{
    let _one=HELPER.lock().unwrap_or_else(|e|e.into_inner());
    let root=card_root();
    let Some(script)=helper(Path::new("/etc/wifi/wifi_init.sh"),&root) else {return Err("Wi-Fi helper missing: System/brick_wifi.sh".into())};
    // Only slot's own helper knows "ensure" and "dhcp"; a firmware one would just print its usage.
    if matches!(verb,"ensure"|"dhcp") && !script.ends_with("System/brick_wifi.sh"){return Ok(())}
    // Inherit slot's stdout/stderr (Logs/slot.log): brick_wifi.sh moves itself to Logs/wifi.log,
    // and anything else, such as a firmware helper run directly, still leaves a trace.
    eprintln!("wifi: {} {verb}",script.display());
    let mut child=Command::new("/bin/sh").arg(&script).arg(verb).env("SLOT_ROOT",&root).stdin(Stdio::null()).spawn().map_err(|_|"Cannot start Wi-Fi helper")?;
    let start=Instant::now();loop{
        if let Some(status)=child.try_wait().map_err(|_|"Cannot check Wi-Fi helper")?{
            eprintln!("wifi: helper {verb} ended: {status}");
            return if status.success(){Ok(())}else{Err("Could not switch Wi-Fi; see Slot/Logs/wifi.log".into())}
        }
        if cancel.load(Ordering::Relaxed){let _=child.kill();let _=child.wait();eprintln!("wifi: helper {verb} cancelled");return Err("Cancelled".into())}
        if start.elapsed()>Duration::from_secs(30){let _=child.kill();let _=child.wait();eprintln!("wifi: helper {verb} timed out");return Err("Wi-Fi helper timed out; see Slot/Logs/wifi.log".into())}
        thread::sleep(Duration::from_millis(100));
    }
}
fn connect(c:&impl Supplicant,n:&Network,password:Option<String>,cancel:&AtomicBool)->Result<String>{
    let old=c.command("LIST_NETWORKS")?;
    let previous:Vec<(u32,bool,bool)>=old.lines().skip(1).filter_map(|s|{let v:Vec<_>=s.split('\t').collect();Some((v.first()?.parse().ok()?,!v.last()?.contains("[DISABLED]"),v.last()?.contains("[CURRENT]")))}).collect();
    let fresh=password.is_some() || n.saved.is_none();
    let secret=if fresh && n.security==Security::Personal{Some(psk(password.as_deref().unwrap_or(""))?)}else{None};
    let id=if fresh{c.command("ADD_NETWORK")?.parse::<u32>().map_err(|_|"Cannot create Wi-Fi profile")?}else{n.saved.unwrap()};
    let result=(||{
        if fresh{c.ok(&format!("SET_NETWORK {id} ssid {}",hex(&n.ssid)))?;c.ok(&format!("SET_NETWORK {id} key_mgmt {}",if n.security==Security::Open{"NONE"}else{"WPA-PSK"}))?;if let Some(secret)=secret{c.ok(&format!("SET_NETWORK {id} psk {secret}"))?;}}
        c.ok(&format!("SELECT_NETWORK {id}"))?;
        let deadline=Instant::now()+Duration::from_secs(20);
        while Instant::now()<deadline{
            pause(cancel,500)?;let s=c.command("STATUS")?;
            if field(&s,"id").and_then(|s|s.parse::<u32>().ok())==Some(id) && field(&s,"wpa_state")==Some("COMPLETED"){
                // Restore eligibility of previously enabled saved networks.
                for &(old,on,_) in &previous{if on{c.ok(&format!("ENABLE_NETWORK {old}"))?;}}
                // Keep the previous profile until a successful replacement has been saved.
                if c.ok("SAVE_CONFIG").is_err(){return Ok("Connected; firmware could not save profile".into())}
                if fresh{if let Some(old)=n.saved{if old!=id && c.ok(&format!("REMOVE_NETWORK {old}")).is_ok(){let _=c.ok("SAVE_CONFIG");}}}
                return Ok(if field(&s,"ip_address").is_some(){"Connected; network saved"}else{"Connected; waiting for IP address"}.into())
            }
        }Err("Connection failed. Check password / signal".into())
    })();
    if result.is_err(){
        if fresh{let _=c.ok(&format!("REMOVE_NETWORK {id}"));}
        if let Some(&(id,_,_))=previous.iter().find(|(_,_,current)|*current){let _=c.ok(&format!("SELECT_NETWORK {id}"));}else{let _=c.ok("DISCONNECT");}
        for &(id,on,_) in &previous{let _=c.ok(&format!("{}_NETWORK {id}",if on{"ENABLE"}else{"DISABLE"}));}
    }result
}
/// Scan(true) restarts Wi-Fi once if the supplicant refuses to scan.
enum Job{Open,Status,Scan(bool),Power(bool),Connect(Network,Option<String>),Disconnect}
struct Reply{on:bool,status:String,networks:Option<Vec<Network>>}
fn run(job:Job,cancel:&AtomicBool)->Result<Reply>{
    // The firmware starts a supplicant at boot with the radio still blocked: it answers, so
    // Wi-Fi looks on, but cannot scan. Repair that once when the menu opens.
    if let Job::Open=job{
        if Control::open().is_ok(){if let Err(e)=helper_run("ensure",cancel){eprintln!("wifi: ensure: {e}");}}
        return run(Job::Status,cancel)
    }
    if let Job::Power(on)=job{
        service(on,cancel)?;
        if !on{return Ok(Reply{on:false,status:"Wi-Fi off".into(),networks:Some(Vec::new())})}
        for _ in 0..20{if Control::open().is_ok(){break}pause(cancel,250)?;}
        return run(Job::Scan(false),cancel)
    }
    let c=match Control::open(){Ok(c)=>c,Err(e)=>return if matches!(job,Job::Status){Ok(Reply{on:false,status:"Wi-Fi off / service unavailable".into(),networks:None})}else{Err(e)}};
    let s=c.command("STATUS")?;let connected=field(&s,"wpa_state")==Some("COMPLETED");
    let mut r=Reply{on:true,status:if connected{format!("{}  {}",name(&decode(field(&s,"ssid").unwrap_or("Connected"))),field(&s,"ip_address").unwrap_or("Waiting for IP"))}else{"Not connected".into()},networks:None};
    match job{
        Job::Status=>{},Job::Open|Job::Power(_)=>unreachable!(),
        Job::Scan(retry)=>{
            if let Err(e)=c.ok("SCAN"){
                if !retry{return Err(e)}
                // One started under a blocked radio answers but refuses scans until restarted,
                // which is what turning Wi-Fi off and on does.
                eprintln!("wifi: scan refused; restarting Wi-Fi");drop(c);service(false,cancel)?;return run(Job::Power(true),cancel)
            }
            pause(cancel,3500)?;r.networks=Some(scan_rows(&c.command("SCAN_RESULTS")?,&c.command("LIST_NETWORKS")?));if r.networks.as_ref().unwrap().is_empty(){r.status="No networks found. Try scanning again".into();}},
        Job::Connect(n,p)=>{
            r.status=connect(&c,&n,p,cancel)?;
            // Joined but no address yet: make sure a DHCP client is asking, then give the
            // router a moment, so "waiting for IP" is not the last word on a working network.
            if r.status.contains("waiting for IP"){
                if let Err(e)=helper_run("dhcp",cancel){eprintln!("wifi: dhcp: {e}");}
                let deadline=Instant::now()+Duration::from_secs(10);
                while Instant::now()<deadline{
                    pause(cancel,500)?;
                    if let Some(ip)=field(&c.command("STATUS")?,"ip_address"){r.status=format!("Connected as {ip}; network saved");break}
                }
            }
            r.networks=Some(scan_rows(&c.command("SCAN_RESULTS")?,&c.command("LIST_NETWORKS")?));
        },
        Job::Disconnect=>{c.ok("DISCONNECT")?;r.status="Disconnected".into();},
    }Ok(r)
}
struct Worker{rx:mpsc::Receiver<Result<Reply>>,cancel:Arc<AtomicBool>}
/// `typed`: when the last character went in; it shows in clear until SHOW_TYPED has passed.
/// What the keyboard is filling in. `password` holds the text, secret or not.
enum KeyTarget{Wifi(Network),RaUser,RaPassword}
struct Keyboard{target:KeyTarget,secret:bool,password:String,key:usize,page:usize,typed:Option<Instant>}
const SHOW_TYPED:Duration=Duration::from_millis(1200);
/// The password as dots, but for the character just typed.
fn masked(k:&Keyboard)->String{
    let n=k.password.chars().count();
    let shown=k.typed.is_some_and(|t|t.elapsed()<SHOW_TYPED);
    if !k.secret{return k.password.chars().skip(n.saturating_sub(30)).collect()}
    let tail=n.saturating_sub(30);
    k.password.chars().enumerate().skip(tail).map(|(i,c)|if shown && i+1==n{c}else{'*'}).collect()
}
/// Three 10x4 pages, Y cycles them: QWERTY under a number row, its shifted twin (US
/// symbols over the numbers), then the rest of the symbols. The last key is space.
const KEYS:[&str;3]=[
    "1234567890qwertyuiopasdfghjkl-zxcvbnm_. ",
    "!@#$%^&*()QWERTYUIOPASDFGHJKL+ZXCVBNM?, ",
    "1234567890-_=+[]{}\\|;:'\"<>,./?`~!@#$%^& ",
];
const PAGES:[&str;3]=["abc","ABC","#!?"];
/// Rows above the network list: Wi-Fi on/off, scan, disconnect.
const FIXED:usize=3;
/// Wi-Fi is usable for downloads: wlan0 is associated. Cheap enough to check every tick.
pub fn online()->bool{fs::read_to_string("/sys/class/net/wlan0/operstate").is_ok_and(|s|s.trim()=="up")}
/// The Cart Art screen: row 0 fetches art, the rest are games with a label, to delete.
struct ArtList{games:Vec<(Platform,String,PathBuf)>,armed:Option<usize>}
impl ArtList{
    fn load()->Self{
        let mut games:Vec<_>=slot_store::scan(&card_root()).unwrap_or_default().into_iter().filter_map(|c|Some((c.platform,c.stem,c.label?))).collect();
        games.sort_by(|a,b|slot_store::sort_key(&a.1).cmp(&slot_store::sort_key(&b.1)).then((a.0 as u8).cmp(&(b.0 as u8))));
        Self{games,armed:None}
    }
}
/// The RetroAchievements screen: log in, test the connection, show a test unlock, log out.
/// `test`: a pretend unlock waiting for the app to toast it.
struct RaScreen{account:Option<ra::Account>,user:String,password:String,job:Option<mpsc::Receiver<RaDone>>,test:Option<TestUnlock>}
/// A pretend unlock: title, description, points, and a badge already cached on the card.
pub struct TestUnlock{pub title:String,pub description:String,pub points:u32,pub badge:Option<PathBuf>}
/// Any badge cached on the card, so the test toast looks like the real thing.
fn cached_badge(root:&Path)->Option<PathBuf>{
    let mut v:Vec<PathBuf>=fs::read_dir(root.join("System/Cache/Badges")).ok()?.filter_map(|e|Some(e.ok()?.path())).filter(|p|p.extension().is_some_and(|x|x=="png")).collect();
    v.sort();v.into_iter().next()
}
enum RaDone{LoggedIn(Result<(ra::Account,ra::Profile)>),Checked(Result<ra::Profile>)}
fn profile_line(p:&ra::Profile)->String{format!("Connected in {} ms: {}, {} points ({} softcore)",p.ms,p.user,p.score,p.softcore)}
/// The overlay behind HOME > Wi-Fi, Cart Art and RetroAchievements: `list` is set for
/// Cart Art, `ra` for RetroAchievements.
pub struct WifiMenu{worker:Option<Worker>,pub face:Option<TexId>,pub dirty:bool,on:bool,status:String,networks:Vec<Network>,row:usize,keyboard:Option<Keyboard>,refreshed:Instant,art:Option<ArtJob>,fetched:Vec<Fetched>,list:Option<ArtList>,ra:Option<RaScreen>}
impl WifiMenu{
    pub fn new()->Self{let mut s=Self{worker:None,face:None,dirty:true,on:supplicant_up(),status:"Checking Wi-Fi...".into(),networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:None,ra:None};s.start(Job::Open);s}
    pub fn cart_art()->Self{
        let list=ArtList::load();
        let status=format!("{} games have a label. A on one twice deletes it",list.games.len());
        Self{worker:None,face:None,dirty:true,on:true,status,networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:Some(list),ra:None}
    }
    pub fn achievements()->Self{
        let account=ra::load(&card_root());
        let status=match &account{Some(a)=>format!("Logged in as {}",a.user),None=>"Log in with your RetroAchievements account".into()};
        Self{worker:None,face:None,dirty:true,on:true,status,networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:None,ra:Some(RaScreen{account,user:String::new(),password:String::new(),job:None,test:None})}
    }
    pub fn busy(&self)->bool{self.worker.is_some() || self.art.is_some() || self.ra.as_ref().is_some_and(|r|r.job.is_some())}
    pub fn cancel(&mut self){
        if let Some(w)=&self.worker{w.cancel.store(true,Ordering::Relaxed);}
        if let Some(a)=&self.art{a.cancel();}
        // A reply that comes back after this is dropped unread, so nothing gets saved.
        if let Some(r)=&mut self.ra{if r.job.take().is_some(){self.status="Cancelled".into();self.dirty=true;return}}
        if self.busy(){self.status="Cancelling...".into();self.dirty=true;}
    }
    /// Labels saved since the last call, for the library to show straight away.
    pub fn take_fetched(&mut self)->Vec<Fetched>{std::mem::take(&mut self.fetched)}
    /// The test unlock asked for since the last call, for the app to toast.
    pub fn take_test_unlock(&mut self)->Option<TestUnlock>{self.ra.as_mut()?.test.take()}
    fn poll_art(&mut self){
        let Some(job)=&self.art else{return};
        loop{match job.try_recv(){
            Ok(Progress::Step{done,total,name})=>{self.status=format!("Cart art {}/{total}: {name}",done+1);self.dirty=true;}
            Ok(Progress::Got(f))=>self.fetched.push(f),
            Ok(Progress::Done(r))=>{
                self.status=match r{
                    Ok(s) if s.found+s.missing+s.failed==0=>"Every game already has a label".into(),
                    Ok(s)=>format!("Cart art: {} added, {} not found{}",s.found,s.missing,if s.failed>0{format!(", {} failed",s.failed)}else{String::new()}),
                    Err(e)=>e,
                };
                if let Some(l)=&mut self.list{*l=ArtList::load();self.row=self.row.min(l.games.len());}
                self.art=None;self.dirty=true;self.refreshed=Instant::now();return;
            }
            Err(mpsc::TryRecvError::Empty)=>return,
            Err(mpsc::TryRecvError::Disconnected)=>{self.art=None;self.status="Cart art stopped".into();self.dirty=true;return}
        }}
    }
    fn start(&mut self,job:Job){
        if self.busy(){return}let(tx,rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let token=cancel.clone();
        if !matches!(job,Job::Status|Job::Open){self.status=match job{Job::Scan(_)=>"Scanning...",Job::Power(true)=>"Turning Wi-Fi on...",Job::Power(false)=>"Turning Wi-Fi off...",Job::Connect(..)=>"Connecting...",Job::Disconnect=>"Disconnecting...",_=>unreachable!()}.into();}
        thread::spawn(move||{let _=tx.send(run(job,&token));});self.worker=Some(Worker{rx,cancel});self.dirty=true;
    }
    pub fn poll(&mut self){
        self.poll_art();
        self.poll_ra();
        if let Some(w)=&self.worker{match w.rx.try_recv(){
            Ok(r)=>{self.worker=None;match r{Ok(r)=>{self.on=r.on;self.status=r.status;if let Some(n)=r.networks{self.networks=n;}},Err(e)=>self.status=e}self.row=self.row.min(self.networks.len()+FIXED-1);self.dirty=true;self.refreshed=Instant::now();},
            Err(mpsc::TryRecvError::Disconnected)=>{self.worker=None;self.status="Wi-Fi worker stopped".into();self.dirty=true;self.refreshed=Instant::now();},_=>{}
        }}
        // Hide the character just typed once its moment is up.
        if let Some(k)=&mut self.keyboard{if k.typed.is_some_and(|t|t.elapsed()>=SHOW_TYPED){k.typed=None;self.dirty=true;}}
        // Poll only while this menu is open; scanning is always explicit. An address still on
        // its way is checked on every couple of seconds, so it shows as soon as it comes.
        let every=if self.status.contains("for IP"){Duration::from_secs(2)}else{Duration::from_secs(15)};
        if self.list.is_none() && self.ra.is_none() && !self.busy() && self.keyboard.is_none() && self.refreshed.elapsed()>every{self.start(Job::Status);}
    }
    pub fn input(&mut self,a:Action)->bool{
        if self.busy(){if matches!(a,Action::GbaDown(Btn::B)){self.cancel();}return false}
        if !matches!(a,Action::GbaDown(_)|Action::ShelfLeft|Action::ShelfRight|Action::Insert){return false}
        self.dirty=true;
        if self.list.is_some(){return self.art_input(a)}
        if let Some(k)=&mut self.keyboard{
            match a{
                Action::GbaDown(Btn::B)=>self.keyboard=None,Action::GbaDown(Btn::X)=>{k.password.pop();k.typed=None;},Action::GbaDown(Btn::Y)=>k.page=(k.page+1)%3,
                Action::GbaDown(Btn::Up)=>k.key=(k.key+30)%40,Action::GbaDown(Btn::Down)=>k.key=(k.key+10)%40,
                Action::GbaDown(Btn::Left)|Action::ShelfLeft=>k.key=k.key/10*10+(k.key%10+9)%10,
                Action::GbaDown(Btn::Right)|Action::ShelfRight=>k.key=k.key/10*10+(k.key%10+1)%10,
                Action::GbaDown(Btn::A)|Action::Insert=>{if k.password.len()<64{k.password.push(KEYS[k.page].as_bytes()[k.key] as char);k.typed=Some(Instant::now());}},
                Action::GbaDown(Btn::Start)=>{
                    if let KeyTarget::Wifi(_)=k.target{if let Err(e)=psk(&k.password){self.status=e;return false}}
                    let k=self.keyboard.take().unwrap();
                    match k.target{
                        KeyTarget::Wifi(n)=>self.start(Job::Connect(n,Some(k.password))),
                        KeyTarget::RaUser=>if let Some(ra)=&mut self.ra{ra.user=k.password.trim().to_string();},
                        KeyTarget::RaPassword=>if let Some(ra)=&mut self.ra{ra.password=k.password;},
                    }
                },_=>{}
            }return false
        }
        if self.ra.is_some(){return self.ra_input(a)}
        match a{
            Action::GbaDown(Btn::B)=>return true,
            Action::GbaDown(Btn::Up)=>self.row=(self.row+self.networks.len()+FIXED-1)%(self.networks.len()+FIXED),
            Action::GbaDown(Btn::Down)=>self.row=(self.row+1)%(self.networks.len()+FIXED),
            Action::GbaDown(Btn::A)|Action::Insert=>match self.row{0=>self.start(Job::Power(!self.on)),1=>self.start(Job::Scan(true)),2=>self.start(Job::Disconnect),_=>self.choose(false)},
            Action::GbaDown(Btn::X)=>self.choose(true),_=>{}
        }false
    }
    fn art_input(&mut self,a:Action)->bool{
        let Some(list)=&mut self.list else{return true};
        let rows=list.games.len()+1;
        match a{
            Action::GbaDown(Btn::B)=>{if list.armed.take().is_none(){return true}self.status="Kept it".into();}
            Action::GbaDown(Btn::Up)=>{self.row=(self.row+rows-1)%rows;list.armed=None;}
            Action::GbaDown(Btn::Down)=>{self.row=(self.row+1)%rows;list.armed=None;}
            Action::GbaDown(Btn::A)|Action::Insert if self.row==0=>{
                if !online(){self.status="Connect to Wi-Fi first".into();return false}
                self.status="Cart art: fetching game list...".into();
                self.art=Some(ArtJob::start(card_root()));
            }
            Action::GbaDown(Btn::A)|Action::Insert=>{
                let at=self.row-1;let Some((platform,stem,path))=list.games.get(at).cloned()else{return false};
                if list.armed!=Some(at){list.armed=Some(at);self.status=format!("Press A again to delete the label of {stem}");return false}
                list.armed=None;
                match fs::remove_file(&path){
                    Ok(())=>{
                        eprintln!("slot: cart art: deleted {}",path.display());
                        list.games.remove(at);self.row=self.row.min(list.games.len());
                        self.fetched.push(Fetched{platform,stem:stem.clone(),label:None});
                        self.status=format!("Deleted the label of {stem}");
                    }
                    Err(e)=>self.status=format!("Cannot delete label: {e}"),
                }
            }
            _=>{}
        }false
    }
    fn choose(&mut self,edit:bool){
        let Some(n)=self.row.checked_sub(FIXED).and_then(|i|self.networks.get(i)).cloned()else{return};
        if n.security==Security::Unsupported{self.status="Use an open or WPA/WPA2 personal network".into();return}
        if n.security==Security::Personal && (edit || n.saved.is_none()){self.status="Password is case sensitive".into();self.keyboard=Some(Keyboard{target:KeyTarget::Wifi(n),secret:true,password:String::new(),key:0,page:0,typed:None});}else{self.start(Job::Connect(n,None));}
    }
    fn poll_ra(&mut self){
        let Some(ra)=&mut self.ra else{return};
        let Some(job)=&ra.job else{return};
        let done=match job.try_recv(){Ok(d)=>d,Err(mpsc::TryRecvError::Empty)=>return,Err(mpsc::TryRecvError::Disconnected)=>{ra.job=None;self.status="RetroAchievements check stopped".into();self.dirty=true;return}};
        ra.job=None;self.dirty=true;
        self.status=match done{
            RaDone::LoggedIn(Ok((account,profile)))=>match ra::save(&card_root(),&account){
                Ok(())=>{ra.password.clear();ra.account=Some(account);self.row=1;format!("Logged in. {}",profile_line(&profile))}
                Err(e)=>format!("Logged in, but cannot save the login: {e}"),
            },
            RaDone::Checked(Ok(profile))=>profile_line(&profile),
            RaDone::LoggedIn(Err(e))|RaDone::Checked(Err(e))=>e,
        };
    }
    fn ra_input(&mut self,a:Action)->bool{
        let Some(ra)=&mut self.ra else{return true};
        let rows=if ra.account.is_some(){4}else{3};
        match a{
            Action::GbaDown(Btn::B)=>return true,
            Action::GbaDown(Btn::Up)=>self.row=(self.row+rows-1)%rows,
            Action::GbaDown(Btn::Down)=>self.row=(self.row+1)%rows,
            Action::GbaDown(Btn::A)|Action::Insert=>match (&ra.account,self.row){
                (None,0)=>self.keyboard=Some(Keyboard{target:KeyTarget::RaUser,secret:false,password:ra.user.clone(),key:0,page:0,typed:None}),
                (None,1)=>self.keyboard=Some(Keyboard{target:KeyTarget::RaPassword,secret:true,password:String::new(),key:0,page:0,typed:None}),
                (None,_)=>{
                    if ra.user.is_empty() || ra.password.is_empty(){self.status="Enter your username and password first".into();return false}
                    if !online(){self.status="Connect to Wi-Fi first".into();return false}
                    let (user,password)=(ra.user.clone(),ra.password.clone());let(tx,rx)=mpsc::channel();
                    thread::spawn(move||{let _=tx.send(RaDone::LoggedIn(ra::login(&user,&password)));});
                    ra.job=Some(rx);self.status="Logging in...".into();
                }
                (Some(_),0)=>{}
                (Some(account),1)=>{
                    if !online(){self.status="Connect to Wi-Fi first".into();return false}
                    let account=account.clone();let(tx,rx)=mpsc::channel();
                    thread::spawn(move||{let _=tx.send(RaDone::Checked(ra::check(&account)));});
                    ra.job=Some(rx);self.status="Testing the connection...".into();
                }
                (Some(_),2)=>{
                    ra.test=Some(TestUnlock{title:"Test Achievement".into(),description:"Toasts are working".into(),points:5,badge:cached_badge(&card_root())});
                    self.status="Test achievement sent".into();
                }
                (Some(_),_)=>{
                    match ra::forget(&card_root()){
                        Ok(())=>{ra.account=None;self.row=0;self.status="Logged out".into();}
                        Err(e)=>self.status=format!("Cannot log out: {e}"),
                    }
                }
            },
            _=>{}
        }false
    }
    fn render_ra(&self,ra:&RaScreen)->UndoFace{
        let mut p=Painter::new();p.text("RETROACHIEVEMENTS",32,24,576,30.0);p.text(&self.status,32,66,576,18.0);
        let rows:Vec<String>=match &ra.account{
            None=>vec![format!("USERNAME     {}",if ra.user.is_empty(){"-"}else{&ra.user}),format!("PASSWORD     {}",if ra.password.is_empty(){"-".to_string()}else{"*".repeat(ra.password.chars().count().min(20))}),"LOG IN".into()],
            Some(a)=>vec![format!("LOGGED IN AS  {}",a.user),"TEST CONNECTION".into(),"TEST ACHIEVEMENT".into(),"LOG OUT".into()],
        };
        for (i,label) in rows.iter().enumerate(){let y=110+i*41;if i==self.row{p.rect(0,y,640,38,slot_ui::edge());}p.text(label,32,y+1,576,23.0);}
        p.text(if self.busy(){"B CANCEL"}else{"B BACK    A SELECT"},24,430,592,20.0);
        UndoFace{rgba:p.rgba,w:640,h:480}
    }
    fn render_art(&self,list:&ArtList)->UndoFace{
        let mut p=Painter::new();p.text("CART ART",32,24,576,30.0);p.text(&self.status,32,66,576,18.0);
        let rows=list.games.len()+1;let first=if self.row<7{0}else{self.row-6};
        for (line,row) in (first..rows.min(first+7)).enumerate(){let y=110+line*41;if row==self.row{p.rect(0,y,640,38,slot_ui::edge());}
            let label=match row{0=>"GET CART ART".into(),_=>{let (platform,stem,_)=&list.games[row-1];format!("{}{}  {}",if list.armed==Some(row-1){"DELETE?  "}else{""},platform.dir_name(),stem)}};
            p.text(&label,32,y+1,576,23.0);
        }
        p.text(if self.busy(){"B CANCEL"}else if self.row==0{"B BACK    A GET ART    LEFT/RIGHT STYLE"}else if list.armed.is_some(){"B KEEP    A DELETE LABEL"}else{"B BACK    A DELETE LABEL"},24,430,592,20.0);
        UndoFace{rgba:p.rgba,w:640,h:480}
    }
    pub fn render(&self)->UndoFace{
        if self.keyboard.is_none(){
            if let Some(list)=&self.list{return self.render_art(list)}
            if let Some(ra)=&self.ra{return self.render_ra(ra)}
        }
        let mut p=Painter::new();p.text(if self.ra.is_some(){"RETROACHIEVEMENTS"}else{"WI-FI"},32,24,576,30.0);p.text(&self.status,32,66,576,18.0);
        if let Some(k)=&self.keyboard{
            let title=match &k.target{KeyTarget::Wifi(n)=>name(&n.ssid),KeyTarget::RaUser=>"USERNAME".into(),KeyTarget::RaPassword=>"PASSWORD".into()};
            p.text(&title,32,102,576,24.0);p.text(&format!("{}  ({}/64)",masked(k),k.password.len()),32,142,576,21.0);
            for i in 0..40{let x=32+i%10*58;let y=196+i/10*45;if i==k.key{p.rect(x,y,54,41,slot_ui::edge());}let c=&KEYS[k.page][i..i+1];if c==" "{p.text("SPC",x,y+9,54,17.0);}else{p.text(c,x,y+3,54,26.0);}}
            p.text(&format!("A TYPE    X DELETE    Y {}",PAGES.iter().enumerate().map(|(i,n)|if i==k.page{format!("[{n}]")}else{n.to_string()}).collect::<Vec<_>>().join(" / ")),24,392,592,18.0);p.text(if matches!(k.target,KeyTarget::Wifi(_)){"B BACK                    START CONNECT"}else{"B BACK                    START DONE"},24,430,592,20.0);
        }else{
            let first=if self.row<7{0}else{self.row-6};
            for (line,row) in (first..(self.networks.len()+FIXED).min(first+7)).enumerate(){let y=110+line*41;if row==self.row{p.rect(0,y,640,38,slot_ui::edge());}
                let label=match row{0=>format!("WI-FI                          {}",if self.on{"ON"}else{"OFF"}),1=>"SCAN NETWORKS".into(),2=>"DISCONNECT".into(),_=>{let n=&self.networks[row-FIXED];format!("{}  {}  {} dBm",name(&n.ssid),if n.saved.is_some(){"[SAVED]"}else if n.security==Security::Open{"[OPEN]"}else{"[LOCK]"},n.signal)}};p.text(&label,32,y+1,576,23.0);
            }p.text(if self.busy(){"B CANCEL"}else{"B BACK    A SELECT    X EDIT PASSWORD"},24,430,592,20.0);
        }UndoFace{rgba:p.rgba,w:640,h:480}
    }
}
struct Painter{rgba:Vec<u8>}
impl Painter{
    fn new()->Self{let mut s=Self{rgba:vec![0;640*480*4]};s.rect(0,0,640,480,slot_ui::opening());s}
    fn rect(&mut self,x:usize,y:usize,w:usize,h:usize,c:[f32;4]){let c=c.map(|v|(v*255.0)as u8);for yy in y..(y+h).min(480){for xx in x..(x+w).min(640){self.rgba[(yy*640+xx)*4..(yy*640+xx)*4+4].copy_from_slice(&c)}}}
    fn text(&mut self,s:&str,x:usize,y:usize,w:usize,px:f32){let Some(font)=slot_ui::text::label_font()else{return};let mut s=s.to_string();while slot_ui::text::line_width(font,&s,px,0.0)>w as f32 && !s.is_empty(){s.pop();}
        let h=40;let mut face=vec![0;w*h*4];let layout=slot_ui::text::Layout{lines:vec![s],px,tracking:0.0};slot_ui::text::draw_centred(&mut face,w as u32,h as u32,&layout,[238,238,242]);
        for yy in 0..h.min(480-y){for xx in 0..w.min(640-x){let a=(yy*w+xx)*4;let b=((y+yy)*640+x+xx)*4;let alpha=face[a+3]as u32;for ch in 0..3{self.rgba[b+ch]=((face[a+ch]as u32*alpha+self.rgba[b+ch]as u32*(255-alpha))/255)as u8;}}}
    }
}
#[cfg(test)]mod tests{
    use super::*;use std::cell::{Cell,RefCell};
    struct Mock{steps:RefCell<std::collections::VecDeque<(String,Result<String>)>>}
    impl Mock{fn new(steps:Vec<(&str,Result<&str>)>)->Self{Self{steps:RefCell::new(steps.into_iter().map(|(a,b)|(a.into(),b.map(String::from))).collect())}}fn done(&self){assert!(self.steps.borrow().is_empty());}}
    impl Supplicant for Mock{fn command(&self,s:&str)->Result<String>{let(a,b)=self.steps.borrow_mut().pop_front().expect("unexpected command");assert_eq!(a,s);b}}
    fn network()->Network{Network{ssid:b"Test".to_vec(),signal:-40,security:Security::Personal,saved:None}}
    #[test]fn boot_repair_waits_for_a_late_supplicant_then_repairs_once_it_joins(){
        let t=Cell::new(0u64);let runs=Cell::new(0);
        // The supplicant appears 5 s in; a network joins 3 s after the repair.
        boot_repair(||t.get()>=5_000,||runs.get()>0&&t.get()>=8_000,||{runs.set(runs.get()+1);Ok(())},|ms|t.set(t.get()+ms));
        assert_eq!(runs.get(),1,"repaired {} times",runs.get());
    }
    #[test]fn boot_repair_tries_twice_then_gives_up_and_never_without_a_supplicant(){
        let t=Cell::new(0u64);let runs=Cell::new(0);
        boot_repair(||true,||false,||{runs.set(runs.get()+1);Ok(())},|ms|t.set(t.get()+ms));
        assert_eq!(runs.get(),2);
        let runs=Cell::new(0);let t=Cell::new(0u64);
        boot_repair(||false,||false,||{runs.set(runs.get()+1);Ok(())},|ms|t.set(t.get()+ms));
        assert_eq!(runs.get(),0,"repaired with no supplicant");
        assert!(t.get()<=BOOT_WAIT_MS+BOOT_POLL_MS,"waited forever");
    }
    #[test]fn parse_scans(){let n=scan_rows("header\na\t2412\t-80\t[WPA2-PSK-CCMP]\tMy\\x20WiFi\nb\t2412\t-40\t[WPA2-PSK-CCMP]\tMy WiFi\nc\t2412\t-30\t[ESS]\tCafe\nd\t2412\t-20\t[WPA2-EAP-CCMP]\tWork\n","header\n7\tMy WiFi\tany\t[CURRENT]\n");assert_eq!(n.len(),3);assert_eq!(n[0].security,Security::Unsupported);assert_eq!(n[2].signal,-40);assert_eq!(n[2].saved,Some(7));}
    #[test]fn password_bytes(){assert!(psk("short").is_err());assert!(psk("1234567\n").is_err());assert_eq!(psk("Abc\"\\123").unwrap(),"\"Abc\"\\123\"");assert_eq!(psk(&"a".repeat(64)).unwrap(),"a".repeat(64));assert!(psk(&"z".repeat(64)).is_err());}
    #[test]fn helper_prefers_card_then_firmware(){
        let d=tempfile::tempdir().unwrap();let fw=d.path().join("wifi_init.sh");let root=d.path().join("Slot");
        assert_eq!(helper(&fw,&root),None);
        fs::write(&fw,"").unwrap();assert_eq!(helper(&fw,&root),Some(fw.clone()));
        fs::create_dir_all(root.join("System")).unwrap();fs::write(root.join("System/brick_wifi.sh"),"").unwrap();
        assert_eq!(helper(&fw,&root),Some(root.join("System/brick_wifi.sh")));
    }
    #[test]fn all_keys(){for s in KEYS{assert_eq!(s.len(),40);assert_eq!(s.as_bytes()[39],b' ',"space last on every page");}for b in 32..=126{assert!(KEYS.iter().any(|s|s.as_bytes().contains(&b)),"{b}");}}
    #[test]fn the_last_typed_character_shows_briefly(){
        let mut k=Keyboard{target:KeyTarget::Wifi(network()),secret:true,password:"hunter2".into(),key:0,page:0,typed:Some(Instant::now())};
        assert_eq!(masked(&k),"******2");
        k.typed=Some(Instant::now()-SHOW_TYPED);assert_eq!(masked(&k),"*******");
        k.typed=None;assert_eq!(masked(&k),"*******");
        k.password="x".repeat(40)+"y";k.typed=Some(Instant::now());
        let m=masked(&k);assert_eq!(m.chars().count(),30);assert!(m.ends_with("*y"));
    }
    #[test]fn a_username_is_typed_in_clear(){
        let k=Keyboard{target:KeyTarget::RaUser,secret:false,password:"Benni".into(),key:0,page:0,typed:None};
        assert_eq!(masked(&k),"Benni");
    }
    #[test]fn ra_screen_types_the_login_and_needs_both_fields(){
        let mut m=WifiMenu{worker:None,face:None,dirty:true,on:true,status:String::new(),networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:None,ra:Some(RaScreen{account:None,user:String::new(),password:String::new(),job:None,test:None})};
        m.input(Action::GbaDown(Btn::A));assert!(m.keyboard.as_ref().is_some_and(|k|!k.secret));
        // q w e on the letter row
        m.input(Action::GbaDown(Btn::Down));for _ in 0..3{m.input(Action::GbaDown(Btn::A));m.input(Action::GbaDown(Btn::Right));}
        m.input(Action::GbaDown(Btn::Start));assert_eq!(m.ra.as_ref().unwrap().user,"qwe");assert!(m.keyboard.is_none());
        m.input(Action::GbaDown(Btn::Down));m.input(Action::GbaDown(Btn::Down));m.input(Action::GbaDown(Btn::A));
        assert_eq!(m.status,"Enter your username and password first");assert!(!m.busy());
        m.input(Action::GbaDown(Btn::Up));m.input(Action::GbaDown(Btn::A));assert!(m.keyboard.as_ref().is_some_and(|k|k.secret));
        if let Ok(path)=std::env::var("SLOT_RA_PREVIEW"){m.keyboard=None;m.ra.as_mut().unwrap().password="secret".into();let f=m.render();let mut e=png::Encoder::new(fs::File::create(path).unwrap(),640,480);e.set_color(png::ColorType::Rgba);e.set_depth(png::BitDepth::Eight);e.write_header().unwrap().write_image_data(&f.rgba).unwrap();}
        assert!(!m.input(Action::GbaDown(Btn::B)),"B closes the keyboard first");assert!(m.keyboard.is_none());
        assert!(m.input(Action::GbaDown(Btn::B)),"then the screen");
    }
    #[test]fn test_achievement_queues_a_pretend_unlock_with_a_cached_badge(){
        let dir=std::env::temp_dir().join(format!("slot-ra-test-{}",std::process::id()));let _=fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("System/Cache/Badges")).unwrap();fs::write(dir.join("System/Cache/Badges/b.png"),b"").unwrap();fs::write(dir.join("System/Cache/Badges/a.png"),b"").unwrap();
        assert_eq!(cached_badge(&dir),Some(dir.join("System/Cache/Badges/a.png")));
        assert_eq!(cached_badge(&dir.join("nowhere")),None);let _=fs::remove_dir_all(&dir);
        let account=ra::Account{user:"Benni".into(),token:"t".into()};
        let mut m=WifiMenu{worker:None,face:None,dirty:true,on:true,status:String::new(),networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:None,ra:Some(RaScreen{account:Some(account),user:String::new(),password:String::new(),job:None,test:None})};
        m.input(Action::GbaDown(Btn::Up));assert_eq!(m.row,3,"four rows when logged in, wrapping to LOG OUT");
        m.input(Action::GbaDown(Btn::Up));assert_eq!(m.row,2);
        assert!(m.take_test_unlock().is_none());
        m.input(Action::GbaDown(Btn::A));
        let t=m.take_test_unlock().unwrap();assert_eq!((t.title.as_str(),t.points),("Test Achievement",5));
        assert!(m.take_test_unlock().is_none(),"taken once");assert!(!m.busy());
    }
    #[test]fn qwerty_under_a_number_row(){
        let rows=|p:usize|KEYS[p].as_bytes().chunks(10).map(|r|String::from_utf8_lossy(r).into_owned()).collect::<Vec<_>>();
        assert_eq!(rows(0),["1234567890","qwertyuiop","asdfghjkl-","zxcvbnm_. "]);
        assert_eq!(rows(1)[1..3],["QWERTYUIOP".to_string(),"ASDFGHJKL+".to_string()]);
        for i in 10..40{let (l,u)=(KEYS[0].as_bytes()[i],KEYS[1].as_bytes()[i]);if l.is_ascii_lowercase(){assert_eq!(u,l.to_ascii_uppercase(),"shift keeps letters in place");}}
    }
    #[test]fn failed_setup_restores_previous_connection(){let m=Mock::new(vec![("LIST_NETWORKS",Ok("header\n3\tOld\tany\t[CURRENT]\n4\tOther\tany\t[DISABLED]")),("ADD_NETWORK",Ok("8")),("SET_NETWORK 8 ssid 54657374",Err("rejected".into())),("REMOVE_NETWORK 8",Ok("OK")),("SELECT_NETWORK 3",Ok("OK")),("ENABLE_NETWORK 3",Ok("OK")),("DISABLE_NETWORK 4",Ok("OK"))]);assert!(connect(&m,&network(),Some("password".into()),&AtomicBool::new(false)).is_err());m.done();}
    #[test]fn cancelled_connect_removes_unsaved_profile(){let m=Mock::new(vec![("LIST_NETWORKS",Ok("header")),("ADD_NETWORK",Ok("8")),("SET_NETWORK 8 ssid 54657374",Ok("OK")),("SET_NETWORK 8 key_mgmt WPA-PSK",Ok("OK")),("SET_NETWORK 8 psk \"password\"",Ok("OK")),("SELECT_NETWORK 8",Ok("OK")),("REMOVE_NETWORK 8",Ok("OK")),("DISCONNECT",Ok("OK"))]);assert_eq!(connect(&m,&network(),Some("password".into()),&AtomicBool::new(true)).unwrap_err(),"Cancelled");m.done();}
    #[test]fn successful_connect_saves_only_after_association(){let m=Mock::new(vec![("LIST_NETWORKS",Ok("header")),("ADD_NETWORK",Ok("8")),("SET_NETWORK 8 ssid 54657374",Ok("OK")),("SET_NETWORK 8 key_mgmt WPA-PSK",Ok("OK")),("SET_NETWORK 8 psk \"password\"",Ok("OK")),("SELECT_NETWORK 8",Ok("OK")),("STATUS",Ok("id=8\nwpa_state=COMPLETED\nip_address=192.168.1.2")),("SAVE_CONFIG",Ok("OK"))]);assert_eq!(connect(&m,&network(),Some("password".into()),&AtomicBool::new(false)).unwrap(),"Connected; network saved");m.done();}
    #[test]#[ignore="Requires a Linux runtime allowing AF_UNIX datagram sockets"]fn socket_roundtrip(){let dir=tempfile::tempdir().unwrap();let remote=dir.path().join("server");let server=UnixDatagram::bind(&remote).unwrap();let t=thread::spawn(move||{let mut b=[0;64];let(n,peer)=server.recv_from(&mut b).unwrap();assert_eq!(&b[..n],b"PING");server.send_to(b"PONG\n",peer.as_pathname().unwrap()).unwrap();});let c=Control::at(&remote).unwrap();let local=c.local.clone();assert_eq!(fs::metadata(&local).unwrap().permissions().mode()&0o777,0o600);assert_eq!(c.command("PING").unwrap(),"PONG");drop(c);assert!(!local.exists());t.join().unwrap();}
    #[test]fn art_screen_deletes_only_after_a_second_a(){
        let d=tempfile::tempdir().unwrap();let label=d.path().join("Emerald.png");fs::write(&label,b"png").unwrap();
        let games=vec![(Platform::Gba,"Emerald".to_string(),label.clone()),(Platform::Gb,"Tetris".to_string(),d.path().join("Tetris.png"))];
        let mut m=WifiMenu{worker:None,face:None,dirty:true,on:true,status:String::new(),networks:Vec::new(),row:0,keyboard:None,refreshed:Instant::now(),art:None,fetched:Vec::new(),list:Some(ArtList{games,armed:None}),ra:None};
        m.input(Action::GbaDown(Btn::Down));m.input(Action::GbaDown(Btn::A));
        assert!(label.exists(),"first A only asks");
        if let Ok(path)=std::env::var("SLOT_ART_PREVIEW"){let f=m.render();let mut e=png::Encoder::new(fs::File::create(path).unwrap(),640,480);e.set_color(png::ColorType::Rgba);e.set_depth(png::BitDepth::Eight);e.write_header().unwrap().write_image_data(&f.rgba).unwrap();}
        assert!(!m.input(Action::GbaDown(Btn::B)),"B disarms instead of closing");assert!(label.exists());
        m.input(Action::GbaDown(Btn::A));m.input(Action::GbaDown(Btn::A));
        assert!(!label.exists());assert_eq!(m.take_fetched(),vec![Fetched{platform:Platform::Gba,stem:"Emerald".into(),label:None}]);
        assert_eq!(m.list.as_ref().unwrap().games.len(),1);
        assert!(m.input(Action::GbaDown(Btn::B)),"B closes once nothing is armed");
    }
    #[test]fn render_fixture(){let s=WifiMenu{worker:None,face:None,dirty:true,on:true,status:"Password is case sensitive".into(),networks:Vec::new(),row:0,keyboard:Some(Keyboard{target:KeyTarget::Wifi(network()),secret:true,password:"Test".into(),key:0,page:0,typed:Some(Instant::now())}),refreshed:Instant::now(),art:None,fetched:Vec::new(),list:None,ra:None};
        let mut s=s;
        // SLOT_WIFI_PREVIEW=<dir>: one picture per keyboard page.
        for page in 0..KEYS.len(){
            if let Some(k)=&mut s.keyboard{k.page=page;k.key=12;}
            let f=s.render();assert_eq!(f.rgba.len(),640*480*4);
            if let Ok(dir)=std::env::var("SLOT_WIFI_PREVIEW"){let mut e=png::Encoder::new(fs::File::create(Path::new(&dir).join(format!("keyboard-{page}.png"))).unwrap(),640,480);e.set_color(png::ColorType::Rgba);e.set_depth(png::BitDepth::Eight);e.write_header().unwrap().write_image_data(&f.rgba).unwrap();}
        }
    }
}
