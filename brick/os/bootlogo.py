"""Slot OS's boot logo, which u-boot shows until Slot's first frame: 1024x768, 32-bit BMP with
a V5 header, rows bottom-up (the format TrimUI's u-boot shows).

brick/os/bootlogo.png, if there is one, centred on black at its own size (made smaller only if it
is bigger than the screen); else the
"slot." wordmark in Slot's own font (Open Sans Bold), white on black.
Usage: python3 bootlogo.py <out.bmp>"""
import os, struct, sys
from PIL import Image, ImageDraw, ImageFont, ImageOps

W, H = 1024, 768
HERE = os.path.dirname(os.path.abspath(__file__))
PICTURE = os.path.join(HERE, 'bootlogo.png')
FONT = os.path.join(HERE, '../../crates/slot-ui/assets/label.ttf')

screen = Image.new('RGB', (W, H), (0, 0, 0))
if os.path.exists(PICTURE):
    pic = Image.open(PICTURE).convert('RGBA')
    if pic.width > W or pic.height > H:
        pic = ImageOps.contain(pic, (W, H), Image.LANCZOS)
    screen.paste(pic, ((W - pic.width) // 2, (H - pic.height) // 2), pic)
else:
    font = ImageFont.truetype(FONT, 160)
    draw = ImageDraw.Draw(screen)
    left, top, right, bottom = draw.textbbox((0, 0), 'slot.', font=font)
    draw.text(((W - (right - left)) // 2 - left, (H - (bottom - top)) // 2 - top), 'slot.',
              font=font, fill=(0xf6, 0xf4, 0xef))

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
