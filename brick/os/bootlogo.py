"""Slot OS's boot logo for u-boot: 1024x768, 32-bit BMP with a V5 header (the format TrimUI's
u-boot shows): a teal cart on dark grey, for now. Usage: python3 bootlogo.py <out.bmp>"""
import struct, sys

W, H = 1024, 768
BG, CART = (0x14, 0x14, 0x16), (0x1f, 0xa8, 0x9c)
cx, cy, cw, ch = W // 2, H // 2, 200, 240
rows = []
for y in range(H - 1, -1, -1):  # bottom-up
    inside = abs(y - cy) < ch // 2
    lo, hi = cx - cw // 2, cx + cw // 2
    r, g, b = BG
    row = bytes((b, g, r, 255)) * lo
    if inside:
        r, g, b = CART
    row += bytes((b, g, r, 255)) * (hi - lo)
    r, g, b = BG
    row += bytes((b, g, r, 255)) * (W - hi)
    rows.append(row)
pixels = b''.join(rows)
v5 = struct.pack('<IiiHHIIiiII', 124, W, H, 1, 32, 3, len(pixels), 2835, 2835, 0, 0)
v5 += struct.pack('<IIII', 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)
v5 += b'BGRs' + bytes(36) + struct.pack('<III', 0, 0, 0) + struct.pack('<IIII', 4, 0, 0, 0)
head = b'BM' + struct.pack('<IHHI', 14 + 124 + len(pixels), 0, 0, 14 + 124)
open(sys.argv[1], 'wb').write(head + v5 + pixels)
