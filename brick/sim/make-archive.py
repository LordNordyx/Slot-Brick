"""Packs a ROM into a .zip or .7z for the simulator with nothing but the standard library.
Python reads 7z but cannot write one, so this writes the one-file LZMA2 kind 7-Zip makes.
`broken-zip` is a zip whose checksum is wrong, for a cart the Brick must refuse.
Usage: python3 make-archive.py zip|7z|broken-zip ROM OUT [NAME-INSIDE]"""
import lzma
import os
import struct
import sys
import zipfile
import zlib


def number(v):
    """A 7z NUMBER: the first byte's leading ones count the bytes after it."""
    for n in range(8):
        if v < 1 << (7 * (n + 1)):
            first = ((0xFF00 >> n) & 0xFF) | (v >> (8 * n))
            return bytes([first]) + (v & ((1 << (8 * n)) - 1)).to_bytes(n, "little")
    return b"\xff" + v.to_bytes(8, "little")


def seven_zip(data, name):
    # A 64 KiB dictionary: LZMA2's property byte 8 is (2 | 0) << (8 // 2 + 11).
    packed = lzma.compress(data, format=lzma.FORMAT_RAW,
                           filters=[{"id": lzma.FILTER_LZMA2, "dict_size": 1 << 16}])
    names = (name + "\0").encode("utf-16-le")
    header = b"".join([
        b"\x01", b"\x04",                                 # Header, MainStreamsInfo
        b"\x06", number(0), number(1),                    # PackInfo: from 0, one stream
        b"\x09", number(len(packed)), b"\x00",            #   of this size
        b"\x07", b"\x0b", number(1), b"\x00",             # UnpackInfo: one folder, here
        number(1), b"\x21\x21", number(1), b"\x08",       #   one coder, LZMA2, its property
        b"\x0c", number(len(data)), b"\x00",              #   unpacking to this size
        b"\x08", b"\x0a", b"\x01", struct.pack("<I", zlib.crc32(data)), b"\x00",  # its CRC
        b"\x00",                                          # end of MainStreamsInfo
        b"\x05", number(1),                               # FilesInfo: one file
        b"\x11", number(len(names) + 1), b"\x00", names,  #   its name
        b"\x00", b"\x00",                                 # end of FilesInfo, of Header
    ])
    start = struct.pack("<QQI", len(packed), len(header), zlib.crc32(header))
    signature = b"7z\xbc\xaf\x27\x1c\x00\x04" + struct.pack("<I", zlib.crc32(start))
    return signature + start + packed + header


def zipped(data, name, out):
    info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
    with zipfile.ZipFile(out, "w") as z:
        z.writestr(info, data, zipfile.ZIP_DEFLATED)


def main():
    kind, rom, out = sys.argv[1:4]
    name = sys.argv[4] if len(sys.argv) > 4 else os.path.basename(rom)
    data = open(rom, "rb").read()
    if kind == "7z":
        open(out, "wb").write(seven_zip(data, name))
        return
    zipped(data, name, out)
    if kind == "broken-zip":
        z = bytearray(open(out, "rb").read())
        z[z.find(b"PK\x01\x02") + 16] ^= 0xFF
        open(out, "wb").write(z)


main()
