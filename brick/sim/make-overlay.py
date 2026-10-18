"""A stand-in overlay for the simulator, made from nothing: a 1024x768 frame in two greys with a
clear window the Game Boy's shape, so a scenario can lay an overlay over a game without shipping
anyone's artwork. Usage: python3 make-overlay.py OUT.png"""
import struct
import sys
import zlib

W, H = 1024, 768
# The window: 10:9 like a Game Boy screen, centred a little high.
WX, WY, WW, WH = 160, 40, 704, 634

rows = bytearray()
for y in range(H):
    rows.append(0)  # no filter
    for x in range(W):
        if WX <= x < WX + WW and WY <= y < WY + WH:
            rows += bytes((0, 0, 0, 0))
        elif WX - 12 <= x < WX + WW + 12 and WY - 12 <= y < WY + WH + 12:
            rows += bytes((60, 64, 72, 255))
        else:
            rows += bytes((120, 40, 140, 255) if (x // 32 + y // 32) % 2 else (90, 30, 110, 255))


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
with open(sys.argv[1], "wb") as f:
    f.write(png)
