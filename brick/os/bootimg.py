"""Make Slot OS's boot.img (Android boot image, version 0, as TrimUI's u-boot reads it): the
kernel and load addresses of a boot.img we are given (boot.fex from TrimUI's firmware), with
our initramfs in place of its own.

Usage: python3 bootimg.py <their boot.img> <initramfs.gz> <out boot.img>
"""
import hashlib, struct, sys

src, ramdisk_path, out = sys.argv[1:4]
img = open(src, 'rb').read()
assert img[:8] == b'ANDROID!', 'not an Android boot image'
kernel_size, kernel_addr, _, ramdisk_addr, second_size, second_addr, tags_addr, page = \
    struct.unpack_from('<8I', img, 8)
assert second_size == 0, 'a second stage is not expected'
pad = lambda n: (n + page - 1) // page * page
kernel = img[page:page + kernel_size]
ramdisk = open(ramdisk_path, 'rb').read()

header = bytearray(img[:page])
struct.pack_into('<I', header, 16, len(ramdisk))
# u-boot's bootargs (env.txt) are what the kernel sees; this line only names the image.
cmdline = b'rdinit=/init init=/init loglevel=1 quiet'
header[64:64 + 512] = cmdline.ljust(512, b'\0')
sha = hashlib.sha1()
for part in (kernel, ramdisk, b''):
    sha.update(part)
    sha.update(struct.pack('<I', len(part)))
header[576:576 + 32] = sha.digest().ljust(32, b'\0')

with open(out, 'wb') as f:
    f.write(header)
    f.write(kernel.ljust(pad(kernel_size), b'\0'))
    f.write(ramdisk.ljust(pad(len(ramdisk)), b'\0'))
print(f'boot.img: kernel {kernel_size} bytes, initramfs {len(ramdisk)} bytes')
