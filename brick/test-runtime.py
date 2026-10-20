from pathlib import Path
import tempfile, subprocess, os, signal
source=Path(__file__).resolve().parent/'card/Slot/System/launch.sh'
for code, command, success in [(0,None,True),(1,None,False),(64,'poweroff',True),(65,'reboot',True),(64,'poweroff',False)]:
 with tempfile.TemporaryDirectory() as td:
  p=Path(td);sys=p/'Slot/System';sys.mkdir(parents=True);cmd=p/'commands';cmd.mkdir();fw=p/'firmware';fw.mkdir();js=p/'js0';js.touch()
  cpu=p/'cpu';cpu.mkdir()
  original={'scaling_min_freq':'408000','scaling_max_freq':'1800000','scaling_governor':'userspace','scaling_setspeed':'1608000'}
  for name,value in original.items():(cpu/name).write_text(value)
  def script(path,text):path.write_text('#!/bin/sh\n'+text);path.chmod(0o755)
  script(cmd/'uname','echo aarch64\n');script(cmd/'pidof','exit 1\n');script(cmd/'sync','exit 0\n')
  for action in ['poweroff','reboot']:
   script(cmd/action, f'echo {action} >> "{p}/actions"\nexit {0 if success else 1}\n')
  script(fw/'trimui_inputd','exec sleep 20\n')
  script(sys/'slot',f'[ "$1" != --init-root ] || exit 0\necho 816000 > "{cpu}/scaling_max_freq"\necho schedutil > "{cpu}/scaling_governor"\necho frontend >> "{p}/actions"\nexit {code}\n')
  script(sys/'diagnose.sh','exit 0\n')
  s=source.read_text().replace('/sys/devices/system/cpu/cpu0/cpufreq',str(p/'cpu')).replace('/usr/trimui/bin',str(fw)).replace('/dev/input/js0',str(js))
  s=s.replace('export PATH='+str(fw)+':/usr/sbin:/sbin:$PATH','export PATH='+str(cmd)+':$PATH')
  (sys/'launch.sh').write_text(s)
  proc=subprocess.Popen(['sh',str(sys/'launch.sh')],start_new_session=True)
  try:
   if command and success:
    try:proc.wait(timeout=0.5);raise AssertionError('successful power request returned to launcher')
    except subprocess.TimeoutExpired:pass
   else:assert proc.wait(timeout=3)==(0 if success else 1)
   lines=(p/'actions').read_text().splitlines();assert lines==(['frontend',command] if command else ['frontend'])
   for name,value in original.items():assert (cpu/name).read_text().strip()==value
   assert 'Started trimui_inputd' in (p/'Slot/Logs/slot.log').read_text()
  finally:
   try:os.killpg(proc.pid,signal.SIGKILL)
   except ProcessLookupError:pass
   proc.wait()
# A frontend killed by a signal leaves its log in Logs/crash (newest five kept).
with tempfile.TemporaryDirectory() as td:
 p=Path(td);sys=p/'Slot/System';sys.mkdir(parents=True);cmd=p/'commands';cmd.mkdir();fw=p/'firmware';fw.mkdir();js=p/'js0';js.touch()
 def script(path,text):path.write_text('#!/bin/sh\n'+text);path.chmod(0o755)
 script(cmd/'uname','echo aarch64\n');script(cmd/'pidof','exit 1\n');script(cmd/'sync','exit 0\n')
 script(fw/'trimui_inputd','exec sleep 20\n');script(sys/'diagnose.sh','exit 0\n')
 script(sys/'slot','[ "$1" != --init-root ] || exit 0\necho "slot: crash: SIGSEGV in thread slot-emu"\nkill -SEGV $$\n')
 crash=p/'Slot/Logs/crash';crash.mkdir(parents=True)
 for i in range(6):(crash/f'slot-2000-01-0{i+1}_00-00-00.log').write_text('old');os.utime(crash/f'slot-2000-01-0{i+1}_00-00-00.log',(i+1,i+1))
 s=source.read_text().replace('/usr/trimui/bin',str(fw)).replace('/dev/input/js0',str(js)).replace('/sys/devices/system/cpu/cpu0/cpufreq',str(p/'nocpu'))
 s=s.replace('export PATH='+str(fw)+':/usr/sbin:/sbin:$PATH','export PATH='+str(cmd)+':$PATH')
 (sys/'launch.sh').write_text(s)
 proc=subprocess.Popen(['sh',str(sys/'launch.sh')],start_new_session=True)
 try:
  assert proc.wait(timeout=5)>=128
  kept=sorted(crash.iterdir());assert len(kept)==5,kept
  new=[f for f in kept if not f.name.startswith('slot-2000')];assert len(new)==1,kept
  assert 'slot: crash: SIGSEGV' in new[0].read_text()
  assert not (crash/'slot-2000-01-01_00-00-00.log').exists() and not (crash/'slot-2000-01-02_00-00-00.log').exists()
 finally:
  try:os.killpg(proc.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  proc.wait()
# Exit 66, an update swapped in: Slot starts again. And an update whose Slot never showed a
# frame (.slot-update/pending still there) has the files it replaced put back, and the old
# Slot starts instead.
for rollback in [False,True]:
 with tempfile.TemporaryDirectory() as td:
  p=Path(td);sys=p/'Slot/System';sys.mkdir(parents=True);cmd=p/'commands';cmd.mkdir();fw=p/'firmware';fw.mkdir();js=p/'js0';js.touch()
  def script(path,text):path.write_text('#!/bin/sh\n'+text);path.chmod(0o755)
  script(cmd/'uname','echo aarch64\n');script(cmd/'pidof','exit 1\n');script(cmd/'sync','exit 0\n')
  script(fw/'trimui_inputd','exec sleep 20\n');script(sys/'diagnose.sh','exit 0\n')
  if rollback:
   old=p/'.slot-update/old/Slot/System';old.mkdir(parents=True);(p/'.slot-update/pending').touch()
   script(old/'slot',f'[ "$1" != --init-root ] || exit 0\necho old >> "{p}/actions"\nexit 0\n')
   (p/'.slot-update/old/Slot/README.md').write_text('old readme');(p/'Slot/README.md').write_text('new readme')
   script(sys/'slot',f'[ "$1" != --init-root ] || exit 0\necho new >> "{p}/actions"\nexit 1\n')
  else:
   script(sys/'slot',f'[ "$1" != --init-root ] || exit 0\necho new >> "{p}/actions"\n[ -f "{p}/again" ] && exit 0\ntouch "{p}/again"\nexit 66\n')
  s=source.read_text().replace('/usr/trimui/bin',str(fw)).replace('/dev/input/js0',str(js)).replace('/sys/devices/system/cpu/cpu0/cpufreq',str(p/'nocpu'))
  s=s.replace('export PATH='+str(fw)+':/usr/sbin:/sbin:$PATH','export PATH='+str(cmd)+':$PATH')
  (sys/'launch.sh').write_text(s)
  proc=subprocess.Popen(['sh',str(sys/'launch.sh')],start_new_session=True)
  try:
   assert proc.wait(timeout=5)==0
   lines=(p/'actions').read_text().splitlines()
   assert lines==(['new','old'] if rollback else ['new','new']),lines
   if rollback:
    assert not (p/'.slot-update').exists()
    assert (p/'Slot/README.md').read_text()=='old readme'
    assert 'putting the previous version back' in (p/'Slot/Logs/slot.previous.log').read_text()
  finally:
   try:os.killpg(proc.pid,signal.SIGKILL)
   except ProcessLookupError:pass
   proc.wait()
print('PASS: cold-start input daemon, normal exit, shutdown/reboot dispatch, no stock fallthrough after successful power request, failed-command recovery, crash log kept, restart after an update, an update that never started put back.')
