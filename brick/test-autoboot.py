from pathlib import Path
import tempfile,shutil,subprocess,os
source=Path(__file__).resolve().parent/'card'
def fixture(td,model='Trimui Brick'):
 p=Path(td);card=p/'card';bin=p/'firmware';cmd=p/'commands';shutil.copytree(source,card);bin.mkdir();cmd.mkdir()
 for f in card.rglob('*'):
  if f.is_file() and (f.suffix=='.sh' or f.name in ['MainUI','keymon']):
   s=f.read_text().replace('/mnt/SDCARD',str(card)).replace('/usr/trimui/bin',str(bin));f.write_text(s)
 subprocess.run(['gcc',str(Path(__file__).parent/'slot-filecheck.c'),'-o',str(card/'Slot/System/slot-filecheck')],check=True)
 (bin/'MainUI').write_text(model+'\n');(bin/'runtrimui.sh').write_text('#!/bin/sh\necho STOCK\n')
 (card/'Slot/System/slot').write_bytes(b"test frontend")
 (card/'Slot/System/launch.sh').write_text('#!/bin/sh\necho SLOT\n[ -z \"$SLOT_READY_FILE\" ] || touch \"$SLOT_READY_FILE\"\nexit 0\n')
 for name in ['sync','sleep']:(cmd/name).write_text('#!/bin/sh\nexit 0\n');(cmd/name).chmod(0o755)
 (p/'mounts').write_text(f'/dev/sd {card} exfat rw 0 0\n')
 env=dict(os.environ,PATH=str(cmd)+':'+os.environ['PATH'],SLOT_HOOK_MOUNTS=str(p/'mounts'));return card,bin,env
def run(f,env):return subprocess.run(['sh',str(f)],env=env,capture_output=True,text=True)
for f in source.rglob('*'):
 if f.is_file() and (f.suffix=='.sh' or f.name in ['MainUI','keymon']):subprocess.run(['sh','-n',str(f)],check=True)
with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td);original=(bin/'runtrimui.sh').read_bytes()
 assert run(card/'trimui/app/MainUI',env).returncode==0
 assert (bin/'runtrimui-slot-backup.sh').read_bytes()==original
 assert not (card/'trimui').exists()
 assert 'SLOT_BRICK_BOOT_V2' in (bin/'runtrimui.sh').read_text()
 assert 'SLOT' in (card/'Slot/Logs/boot.log').read_text()
 assert 'Installed v0.5' in (card/'Slot/Logs/autoboot-status.txt').read_text()
 hook=bin/'runtrimui.sh'
 assert run(hook,env).stdout.strip()=='STOCK'
 log=(card/'Slot/Logs/boot.log').read_bytes()
 (card/'Slot/DISABLE_AUTOBOOT').write_text('')
 assert run(hook,env).stdout.strip()=='STOCK'
 assert (card/'Slot/Logs/boot.log').read_bytes()==log
 (card/'Slot/DISABLE_AUTOBOOT').unlink();(card/'Slot/System/slot').unlink()
 assert run(hook,env).stdout.strip()=='STOCK'
 assert 'SLOT' not in (card/'Slot/Logs/boot.log').read_text()
 assert run(card/'Apps/SlotBootRestore/launch.sh',env).returncode==0
 assert hook.read_bytes()==original
 assert (card/'Slot/DISABLE_AUTOBOOT').exists()
with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td,'Trimui Smart Pro');before=(bin/'runtrimui.sh').read_bytes()
 run(card/'trimui/app/MainUI',env)
 assert (bin/'runtrimui.sh').read_bytes()==before
 assert not (bin/'runtrimui-slot-backup.sh').exists()
with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td);(bin/'runtrimui-slot-backup.sh').write_text('unrelated backup')
 original=(bin/'runtrimui.sh').read_bytes()
 run(card/'trimui/app/MainUI',env)
 # Firmware replaced the startup script after an earlier install: the stale
 # backup is kept aside, the current script becomes the backup, the hook installs.
 assert (bin/'runtrimui-slot-backup.previous.sh').read_text()=='unrelated backup'
 assert (bin/'runtrimui-slot-backup.sh').read_bytes()==original
 assert 'SLOT_BRICK_BOOT_V2' in (bin/'runtrimui.sh').read_text()
 assert run(bin/'runtrimui.sh',env).stdout.strip()=='STOCK'
print('PASS: syntax, first-install backup, installer retirement, direct launch, single fallback, disable marker, missing binary fallback, restore, unsupported model rejection, stale backup after firmware update.')

with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td);original=(bin/'runtrimui.sh').read_bytes()
 # The exact user-reported entry point must install without boot discovery.
 assert run(card/'Apps/Slot/launch.sh',env).returncode==0
 assert (bin/'runtrimui-slot-backup.sh').read_bytes()==original
 assert 'SLOT_BRICK_BOOT_V2' in (bin/'runtrimui.sh').read_text()
 before=(bin/'runtrimui.sh').stat().st_mtime_ns
 assert run(card/'Apps/Slot/launch.sh',env).returncode==0
 assert (bin/'runtrimui.sh').stat().st_mtime_ns==before
 # Failure before first frame retries only once, then returns to stock.
 (card/'Slot/System/slot').write_bytes(b"test frontend")
 (card/'Slot/System/launch.sh').write_text('#!/bin/sh\necho FAIL\nexit 1\n')
 assert run(bin/'runtrimui.sh',env).stdout.strip()=='STOCK'
 assert (card/'Slot/Logs/boot.log').read_text().count('FAIL')==2
print('PASS: stock-menu installation, idempotent hook, bounded startup retry.')

with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td)
 assert run(card/'Apps/Slot/launch.sh',env).returncode==0
 # Simulate a new full card on an already-hooked device.
 shutil.copytree(source/'trimui',card/'trimui')
 assert run(bin/'runtrimui.sh',env).stdout.strip()=='STOCK'
 assert not (card/'trimui').exists()
 assert (card/'Slot/Logs/boot.log').read_text().count('SLOT\n')==1
print('PASS: full card on an already configured Brick retires discovery before Slot, then exits once to stock.')

with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td)
 assert run(card/'trimui/app/MainUI',env).returncode==0
 # The card's log files cannot be written (simulated): Slot must still start.
 for name in ['boot.log','install.log']:
  f=card/'Slot/Logs'/name
  if f.exists(): f.unlink()
  f.mkdir()
 Path('/tmp/slot-boot.log').unlink(missing_ok=True)
 run(bin/'runtrimui.sh',env)
 assert 'SLOT' in Path('/tmp/slot-boot.log').read_text(), Path('/tmp/slot-boot.log').read_text()
 assert 'not writable' in Path('/tmp/slot-boot.log').read_text()
print('PASS: an unwritable card still starts Slot.')

with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td)
 assert run(card/'trimui/app/MainUI',env).returncode==0
 # An older hook already on the Brick is brought up to date on the next Slot start.
 hook=bin/'runtrimui.sh'
 hook.write_text(hook.read_text().replace('Startup hook v5','Startup hook v4 (old)'))
 run(hook,env)
 assert 'Startup hook v5' in hook.read_text()
 assert 'card mounted=yes, Slot folder before mount=yes, AUTOBOOT=yes DISABLE=no boot.sh=yes' in (card/'Slot/Logs/hook.log').read_text()
 assert 'slot autoboot v0.6' in (card/'Slot/Logs/boot.previous.log').read_text() or 'slot autoboot v0.6' in (card/'Slot/Logs/boot.log').read_text()
print('PASS: hook self-update and hook log.')

with tempfile.TemporaryDirectory() as td:
 card,bin,env=fixture(td)
 assert run(card/'trimui/app/MainUI',env).returncode==0
 # Card not mounted yet but a stray Slot folder exists: wait for the mount, then start Slot.
 mounts=Path(env['SLOT_HOOK_MOUNTS']); mounts.write_text('')
 import threading,time
 threading.Timer(1.2,lambda: mounts.write_text(f'/dev/sd {card} exfat rw 0 0\n')).start()
 before=(card/'Slot/Logs/boot.log').read_text() if (card/'Slot/Logs/boot.log').exists() else ''
 env2=dict(env); env2['PATH']=os.environ['PATH']  # real sleep
 t=time.time(); run(bin/'runtrimui.sh',env2); took=time.time()-t
 log=(card/'Slot/Logs/hook.log').read_text().splitlines()[-1]
 assert 'card mounted=yes' in log and 'waited 0 ' not in log, log
 assert 'SLOT' in (card/'Slot/Logs/boot.log').read_text()
print('PASS: hook waits for the card mount, not for a Slot folder.')
