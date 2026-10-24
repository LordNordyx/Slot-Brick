"""Slot OS's boot logo, which u-boot shows until Slot's first frame: "slot." in pixel letters
drawn on a grid (so every edge is sharp), a banded sunset from top to bottom, extruded depth
and scanlines, on black. 1024x768, 32-bit BMP with a V5 header, rows bottom-up (the format
TrimUI's u-boot shows). Usage: python3 bootlogo.py <out.bmp>"""
import struct, sys
from PIL import Image, ImageDraw

W, H = 1024, 768
CELL, GAP, DEPTH = 12, 4, 2  # pixels per grid cell; cells between letters; depth in cells
# 14 rows: 0-3 ascenders, 4-13 x-height. Horizontal strokes 2 cells, vertical 3.
X = [""] * 4
GLYPHS = {
    's': X + [" #########", "##########", "###", "###", "#########",
              " #########", "       ###", "       ###", "##########", "#########"],
    'l': ["###"] * 12 + ["####", " ###"],
    'o': X + [" ########", "##########"] + ["###    ###"] * 6 + ["##########", " ########"],
    't': [""] + [" ###"] * 3 + ["########"] * 2 + [" ###"] * 6 + [" #######", "  ######"],
    '.': [""] * 11 + ["###"] * 3,
}
# One colour per grid row, yellow at the top to blue at the foot; the depth in two purples.
BANDS = ["#fff7a1", "#ffe66d", "#ffcf4a", "#ffb43c", "#ff9636", "#ff7a3d", "#ff5e4f", "#ff4767",
         "#f43b85", "#dc36a3", "#bd35bf", "#9638d6", "#6e3fe4", "#4c4cec"]
DEPTHS = ["#3a1466", "#24104a"]


def cells(text):
    x0 = 0
    for c in text:
        glyph = GLYPHS[c]
        width = max(map(len, glyph))
        for y, row in enumerate(glyph):
            for x, ch in enumerate(row):
                if ch == '#':
                    yield x0 + x, y
        x0 += width + GAP


on = set(cells("slot."))
cols, rows = max(x for x, _ in on) + 1 + DEPTH, len(BANDS) + DEPTH
left, top = (W - cols * CELL) // 2, (H - rows * CELL) // 2
screen = Image.new('RGB', (W, H), (0, 0, 0))
draw = ImageDraw.Draw(screen)


def square(x, y, colour, height=CELL, foot=0):
    px, py = left + x * CELL, top + y * CELL + foot
    draw.rectangle([px, py, px + CELL - 1, py + height - 1], fill=colour)


# The depth, farthest first; then the letters, each cell in its row's band with a darker
# line at its foot.
for k in range(DEPTH, 0, -1):
    for x, y in on:
        square(x + k, y + k, DEPTHS[k - 1])
line = max(1, CELL // 7)
for x, y in on:
    band = BANDS[y]
    square(x, y, band)
    r, g, b = (int(band[i:i + 2], 16) * 78 // 100 for i in (1, 3, 5))
    square(x, y, (r, g, b), height=line, foot=CELL - line)

pixels = b''.join(
    b''.join(bytes((b, g, r, 255)) for r, g, b in
             (screen.getpixel((x, y)) for x in range(W)))
    for y in range(H - 1, -1, -1))
v5 = struct.pack('<IiiHHIIiiII', 124, W, H, 1, 32, 3, len(pixels), 2835, 2835, 0, 0)
v5 += struct.pack('<IIII', 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)
v5 += b'BGRs' + bytes(36) + struct.pack('<III', 0, 0, 0) + struct.pack('<IIII', 4, 0, 0, 0)
head = b'BM' + struct.pack('<IHHI', 14 + 124 + len(pixels), 0, 0, 14 + 124)
with open(sys.argv[1], 'wb') as out:
    out.write(head + v5 + pixels)
