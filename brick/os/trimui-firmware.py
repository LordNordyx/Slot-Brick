"""Takes from TrimUI's firmware release (TG3040_Brick_firmware_*.zip: an Allwinner IMAGEWTY
image, trimui_tg3040.awimg) the two parts Slot OS uses: boot.fex (the boot image with TrimUI's
kernel) and rootfs.fex (their system, ext4, from which make-image.sh takes the drivers, the
GPU libraries and the firmware with debugfs).

Usage: python3 trimui-firmware.py <firmware zip> <out dir>
"""
import os, shutil, struct, sys, zipfile

src, out = sys.argv[1:3]
os.makedirs(out, exist_ok=True)
WANT = {'boot.fex', 'rootfs.fex'}
with zipfile.ZipFile(src) as z:
    name = next(n for n in z.namelist() if n.endswith('.awimg'))
    awimg = os.path.join(out, 'firmware.awimg')
    with z.open(name) as i, open(awimg, 'wb') as o:
        shutil.copyfileobj(i, o, 1 << 20)
with open(awimg, 'rb') as f:
    head = f.read(1024)
    assert head[:8] == b'IMAGEWTY', 'not an IMAGEWTY image'
    count = struct.unpack_from('<I', head, 0x3c)[0]
    # A 1 KiB header per file after the image's own: its name at 0x24, its length at 0x12c
    # and its place in the image at 0x134.
    for k in range(1, count + 1):
        f.seek(1024 * k)
        h = f.read(1024)
        file = h[0x24:0x124].split(b'\0')[0].decode()
        length, at = struct.unpack_from('<I', h, 0x12c)[0], struct.unpack_from('<I', h, 0x134)[0]
        if file in WANT:
            WANT.discard(file)
            f.seek(at)
            with open(os.path.join(out, file), 'wb') as o:
                left = length
                while left:
                    block = f.read(min(left, 1 << 20))
                    o.write(block)
                    left -= len(block)
os.remove(awimg)
assert not WANT, f'not in the firmware: {WANT}'
