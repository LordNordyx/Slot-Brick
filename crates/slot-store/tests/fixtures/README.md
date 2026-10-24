# Archive fixtures

`rom.gb` is 1 KiB made up for these tests: a Game Boy header (title `ZIPTEST`, CGB flag 0x80)
over the bytes `(i * 7) & 0xff`. Every archive here holds it as `rom.gb`, made by the tool a
player might use, so the tests read what those tools really write.

```sh
python3 -I -c "
rom = bytearray((i * 7) & 0xff for i in range(0x400))
rom[0x134:0x13f] = b'ZIPTEST'.ljust(11, b'\0')
rom[0x143] = 0x80
open('rom.gb', 'wb').write(rom)"
```

| File | Made with (macOS, Info-ZIP 3.0, 7-Zip 17.05) | What it is |
| :--- | :--- | :--- |
| `rom.zip` | `zip -X -9 rom.zip rom.gb` | deflated |
| `rom-stored.zip` | `zip -X -0 rom-stored.zip rom.gb` | stored |
| `rom-7zip.zip` | `7z a -tzip rom-7zip.zip rom.gb` | 7-Zip's zip |
| `rom-descriptor.zip` | Python's `zipfile` writing to a stream it cannot seek | sizes in a data descriptor |
| `rom-mac.zip` | `xattr -w com.apple.slot test rom.gb && ditto -c -k --sequesterRsrc rom.gb rom-mac.zip` | with a `__MACOSX/._rom.gb` fork, as Finder's Compress makes |
| `rom-lzma.7z` | `7z a -m0=LZMA rom-lzma.7z rom.gb` | LZMA |
| `rom-lzma2.7z` | `7z a -m0=LZMA2 rom-lzma2.7z rom.gb` | LZMA2, 7-Zip's default |
| `rom-python.7z` | `python3 -I brick/sim/make-archive.py 7z rom.gb rom-python.7z` | the simulator's own 7z writer |
| `rom-ppmd.7z` | `7z a -m0=PPMd rom-ppmd.7z rom.gb` | a method Slot does not unpack |
| `rom-bzip2.zip` | `7z a -tzip -mm=BZip2 rom-bzip2.zip rom.gb` | a method Slot does not unpack |
| `rom-encrypted.zip` | `zip -X -P secret rom-encrypted.zip rom.gb` | password |
| `rom-encrypted.7z` | `7z a -psecret -mhe=off rom-encrypted.7z rom.gb` | password, names visible |
| `rom-hidden.7z` | `7z a -psecret -mhe=on rom-hidden.7z rom.gb` | password, names hidden too |

The descriptor zip:

```sh
python3 -I -c "
import zipfile, io
class Pipe(io.RawIOBase):
    def __init__(s): s.b = bytearray()
    def writable(s): return True
    def write(s, d): s.b += d; return len(d)
p = Pipe()
with zipfile.ZipFile(p, 'w', zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo('rom.gb', (2026, 1, 1, 0, 0, 0)), open('rom.gb', 'rb').read(), zipfile.ZIP_DEFLATED)
open('rom-descriptor.zip', 'wb').write(p.b)"
```
