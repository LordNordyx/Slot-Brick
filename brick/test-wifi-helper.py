#!/usr/bin/env python3
"""Exercise System/brick_wifi.sh against stub firmware tools; never touches host networking."""
import os, pathlib, socket, subprocess, tempfile, time, unittest
SCRIPT = pathlib.Path(__file__).parent / 'card/Slot/System/brick_wifi.sh'

# Binds a control socket and marks the supplicant behind it as alive (wpa_cli stub answers PONG).
BIND = ('python3 -c "import socket,sys,os;os.makedirs(sys.argv[1],exist_ok=True);'
        's=socket.socket(socket.AF_UNIX);s.bind(sys.argv[1]+\'/wlan0\');open(sys.argv[1]+\'/alive\',\'w\')" ')

class WifiHelperTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); p = pathlib.Path(self.tmp.name)
        self.p = p; self.root = p/'Slot'; (self.root/'Logs').mkdir(parents=True)
        self.etc = p/'etc'; self.net = p/'net'; self.varrun = p/'run'; self.run_dir = p/'slotrun'
        self.bin = p/'bin'; self.bin.mkdir(); self.calls = p/'calls'
        (self.net/'wlan0').mkdir(parents=True); self.etc.mkdir()
        # Like the Brick at boot: radio soft-blocked, wlan0 present but down.
        self.rfkill = p/'rfkill'; r = self.rfkill/'rfkill0'; r.mkdir(parents=True)
        (r/'type').write_text('wlan\n'); (r/'soft').write_text('1\n'); (r/'hard').write_text('0\n')
        (self.net/'wlan0/flags').write_text('0x1002\n')
        self.exe('rfkill', 'echo "rfkill $*" >> "$CALLS"\n')
        # ifconfig fails while the radio is blocked, as the real one does.
        self.exe('ifconfig', 'echo "ifconfig $*" >> "$CALLS"\n'
                 'if [ "$2" = up ]; then\n'
                 '  [ "$(cat "$RFKILL/rfkill0/soft")" = 0 ] || { echo "SIOCSIFFLAGS: Operation not possible due to RF-kill"; exit 1; }\n'
                 '  echo 0x1003 > "$NETDIR/$1/flags"\n'
                 'elif [ "$2" = down ] || [ "$3" = down ]; then echo 0x1002 > "$NETDIR/$1/flags"; fi\n')
        # Stub supplicant: record args and expose a control socket like the real one.
        self.exe('wpa_supplicant', 'echo "wpa_supplicant $*" >> "$CALLS"\n' + BIND + '"$SOCKDIR"\n')
        # A DHCP client that would hang if the helper kept our pipes open.
        self.exe('udhcpc', 'echo "udhcpc $*" >> "$CALLS"\nexec sleep 30\n')
        self.exe('wpa_cli', 'echo "wpa_cli $*" >> "$CALLS"\ncase "$*" in *ping) [ -f "$2/alive" ] && echo PONG;; *terminate) rm -f "$2/alive";; esac\nexit 0\n')
        self.exe('pidof', 'exit 1\n')
    def tearDown(self):
        subprocess.run(['pkill', '-f', 'sleep 30'], check=False, stderr=subprocess.DEVNULL)
        self.tmp.cleanup()
    def exe(self, name, text):
        f = self.bin/name; f.write_text('#!/bin/sh\n' + text); f.chmod(0o755)
    def firmware(self, body):
        (self.etc/'wifi').mkdir(exist_ok=True); f = self.etc/'wifi/wifi_init.sh'; f.write_text('echo "firmware $1" >> "$CALLS"\n' + body)
    def helper(self, verb, timeout=15):
        env = dict(os.environ, SLOT_ROOT=str(self.root), SLOT_WIFI_RUN=str(self.run_dir), SLOT_WIFI_ETC=str(self.etc),
                   SLOT_WIFI_NET=str(self.net), SLOT_WIFI_VARRUN=str(self.varrun), SLOT_WIFI_RFKILL=str(self.rfkill),
                   RFKILL=str(self.rfkill), NETDIR=str(self.net), CALLS=str(self.calls),
                   SOCKDIR=str(self.varrun/'wpa_supplicant'), SLOT_WIFI_PATH=str(self.bin), PATH=str(self.bin) + ':/usr/bin:/bin')
        start = time.monotonic()
        # capture_output keeps pipes open like slot's spawn; return proves no daemon holds them.
        r = subprocess.run(['/bin/sh', str(SCRIPT), verb], env=env, capture_output=True, timeout=timeout)
        return r.returncode, time.monotonic() - start
    def lines(self):
        return self.calls.read_text().splitlines() if self.calls.exists() else []
    def log(self):
        return (self.root/'Logs/wifi.log').read_text()
    def soft(self):
        return (self.rfkill/'rfkill0/soft').read_text().strip()

    def test_start_brings_up_supplicant_socket_and_dhcp_without_blocking(self):
        code, took = self.helper('start')
        self.assertEqual(code, 0); self.assertLess(took, 5)
        calls = self.lines()
        self.assertIn('rfkill unblock wifi', calls); self.assertIn('ifconfig wlan0 up', calls)
        conf = self.root/'Config/wpa_supplicant.conf'
        self.assertTrue(any(c.startswith('wpa_supplicant -B -D nl80211 -i wlan0 -c ' + str(conf)) for c in calls), calls)
        self.assertIn('update_config=1', conf.read_text())
        self.assertTrue(any(c.startswith('udhcpc -i wlan0') for c in calls), calls)
        self.assertTrue((self.run_dir/'udhcpc.script').exists())
    def test_radio_unblocked_through_sysfs_without_rfkill_tools(self):
        (self.bin/'rfkill').unlink()
        self.assertEqual(self.helper('start')[0], 0)
        self.assertEqual(self.soft(), '0')
        self.assertIn('radio unblock: rfkill0 soft=0 hard=0', self.log())
    def test_hard_blocked_radio_fails_and_says_so(self):
        (self.bin/'rfkill').unlink(); (self.rfkill/'rfkill0/hard').write_text('1\n')
        # A hard block keeps soft from mattering: model it by refusing the write.
        (self.rfkill/'rfkill0/soft').chmod(0o444)
        self.assertNotEqual(self.helper('start')[0], 0)
        log = self.log(); self.assertIn('hard=1', log); self.assertIn('cannot bring wlan0 up', log)
    def test_existing_supplicant_is_reused(self):
        d = self.varrun/'wpa_supplicant'; d.mkdir(parents=True)
        s = socket.socket(socket.AF_UNIX); s.bind(str(d/'wlan0')); (d/'alive').touch()
        try:
            self.assertEqual(self.helper('start')[0], 0)
            self.assertFalse(any(c.startswith('wpa_supplicant') for c in self.lines()))
        finally:
            s.close()
    def test_stale_socket_is_replaced(self):
        d = self.varrun/'wpa_supplicant'; d.mkdir(parents=True)
        s = socket.socket(socket.AF_UNIX); s.bind(str(d/'wlan0')); s.close()
        self.assertEqual(self.helper('start')[0], 0)
        self.assertTrue(any(c.startswith('wpa_supplicant -B') for c in self.lines()), self.lines())
    def test_stock_profiles_used_when_socket_reachable(self):
        (self.etc/'wifi').mkdir(); (self.etc/'wifi/wpa_supplicant.conf').write_text('ctrl_interface=/etc/wifi/sockets\nupdate_config=1\n')
        self.helper('start', timeout=20)
        self.assertTrue(any('-c ' + str(self.etc/'wifi/wpa_supplicant.conf') in c for c in self.lines()), self.lines())
    def test_firmware_helper_used_when_it_brings_wifi_up(self):
        self.firmware('ifconfig wlan0 up\n' + BIND + '"$SOCKDIR"\n')
        self.assertEqual(self.helper('start')[0], 0)
        calls = self.lines()
        self.assertIn('firmware start', calls)
        self.assertFalse(any(c.startswith('wpa_supplicant') for c in calls), calls)
        self.assertIn('firmware helper start exited 0', self.log())
    def test_brick_firmware_helper_that_lies_falls_back(self):
        # What the Brick's wifi_init.sh did: rfkill.elf missing, radio left blocked by the
        # previous stop, ifconfig refused, exit 0 anyway, stale socket left behind.
        (self.bin/'rfkill').unlink()
        d = self.varrun/'wpa_supplicant'; d.mkdir(parents=True)
        s = socket.socket(socket.AF_UNIX); s.bind(str(d/'wlan0')); s.close()
        self.firmware('rfkill.elf unblock wifi\n'
                      'echo 1 > "$RFKILL/rfkill0/soft"\n'
                      'ifconfig wlan0 up\necho "======wlan0 up successed!======"\nexit 0\n')
        code, took = self.helper('start', timeout=20)
        self.assertEqual(code, 0, self.log())
        self.assertEqual(self.soft(), '0')
        self.assertIn('firmware helper did not bring Wi-Fi up; using slot helper', self.log())
        self.assertTrue(any(c.startswith('wpa_supplicant -B') for c in self.lines()), self.lines())
    def test_failing_firmware_helper_falls_back_and_is_logged(self):
        self.firmware('echo "firmware says no"; exit 3\n')
        self.assertEqual(self.helper('start')[0], 0)
        calls = self.lines()
        self.assertIn('firmware start', calls)
        self.assertTrue(any(c.startswith('wpa_supplicant -B') for c in calls), calls)
        log = self.log()
        self.assertIn('firmware says no', log); self.assertIn('firmware helper start exited 3', log)
    def test_missing_interface_fails(self):
        (self.net/'wlan0/flags').unlink(); (self.net/'wlan0').rmdir()
        code, took = self.helper('start')
        self.assertNotEqual(code, 0); self.assertLess(took, 10)
        self.assertIn('no wlan0', self.log())
    def test_stop_turns_everything_off(self):
        self.assertEqual(self.helper('start')[0], 0)
        self.assertEqual(self.helper('stop')[0], 0)
        calls = self.lines()
        self.assertTrue(any(c.startswith('wpa_cli -p') and c.endswith('terminate') for c in calls), calls)
        self.assertIn('rfkill block wifi', calls)
        self.assertEqual(self.soft(), '1')
        self.assertFalse((self.run_dir/'udhcpc.pid').exists())
    def test_stop_runs_firmware_then_own(self):
        self.firmware('exit 0\n')
        self.assertEqual(self.helper('stop')[0], 0)
        calls = self.lines()
        self.assertEqual(calls[0], 'firmware stop'); self.assertIn('rfkill block wifi', calls)
    def test_radio_only_powers_interface(self):
        self.assertEqual(self.helper('radio')[0], 0)
        self.assertEqual(self.lines(), ['rfkill unblock wifi', 'ifconfig wlan0 up'])
        self.assertEqual(self.soft(), '0')
    def test_radio_never_delegates(self):
        self.firmware('exit 0\n')
        self.assertEqual(self.helper('radio')[0], 0)
        self.assertNotIn('firmware radio', self.lines())
    def test_unwritable_log_falls_back_to_tmp(self):
        (self.root/'Logs').chmod(0o500)
        try:
            self.assertEqual(self.helper('radio')[0], 0)
        finally:
            (self.root/'Logs').chmod(0o700)
        self.assertFalse((self.root/'Logs/wifi.log').exists())
    def test_ensure_restarts_a_supplicant_started_with_the_radio_blocked(self):
        # The Brick at boot: firmware supplicant answering, radio blocked, wlan0 down. It
        # never recovers from that, so ensure restarts Wi-Fi like an off/on toggle.
        (self.bin/'rfkill').unlink()
        d = self.varrun/'wpa_supplicant'; d.mkdir(parents=True)
        s = socket.socket(socket.AF_UNIX); s.bind(str(d/'wlan0')); (d/'alive').touch()
        try:
            code, took = self.helper('ensure', timeout=20)
        finally:
            s.close()
        self.assertEqual(code, 0, self.log())
        self.assertEqual(self.soft(), '0')
        self.assertEqual((self.net/'wlan0/flags').read_text().strip(), '0x1003')
        calls = self.lines()
        self.assertIn('supplicant running with the radio off; restarting Wi-Fi', self.log())
        self.assertTrue(any(c.endswith('terminate') for c in calls), calls)
        self.assertTrue(any(c.startswith('wpa_supplicant -B') for c in calls), calls)
        self.assertTrue(any(c.startswith('udhcpc -i wlan0') for c in calls), calls)
    def test_ensure_keeps_a_healthy_supplicant(self):
        (self.rfkill/'rfkill0/soft').write_text('0\n'); (self.net/'wlan0/flags').write_text('0x1003\n')
        d = self.varrun/'wpa_supplicant'; d.mkdir(parents=True)
        s = socket.socket(socket.AF_UNIX); s.bind(str(d/'wlan0')); (d/'alive').touch()
        try:
            self.assertEqual(self.helper('ensure')[0], 0, self.log())
        finally:
            s.close()
        calls = self.lines()
        self.assertFalse(any(c.startswith('wpa_supplicant') or c.endswith('terminate') for c in calls), calls)
        self.assertTrue(any(c.startswith('udhcpc -i wlan0') for c in calls), calls)
    def test_ensure_without_a_supplicant_does_nothing(self):
        self.assertEqual(self.helper('ensure')[0], 1)
        self.assertEqual(self.soft(), '1')
    def test_dhcp_only_starts_a_dhcp_client(self):
        # After joining a network: an address is asked for, and Wi-Fi is otherwise left alone.
        code, took = self.helper('dhcp')
        self.assertEqual(code, 0, self.log()); self.assertLess(took, 5)
        calls = self.lines()
        self.assertTrue(any(c.startswith('udhcpc -i wlan0') for c in calls), calls)
        self.assertFalse(any(c.startswith(('wpa_supplicant', 'rfkill', 'ifconfig')) for c in calls), calls)
        self.assertEqual(self.soft(), '1', 'the radio was touched')
    def test_bad_verb(self):
        self.assertEqual(self.helper('restart')[0], 2)

if __name__ == '__main__':
    unittest.main()
