#!/usr/bin/env python3
"""Zip built card files (brick/docker-build.sh's output) as the SD card download.

Everything goes under SD-CARD/, empty folders included (Games/NES and the like must exist on
the card), and file modes are kept, so Slot/System/slot stays executable.

Usage: python3 brick/zip-card.py [dist-brick] [slot-stock-dev.zip]
"""
import os, stat, sys, zipfile

src = sys.argv[1] if len(sys.argv) > 1 else 'dist-brick'
out = sys.argv[2] if len(sys.argv) > 2 else 'slot-stock-dev.zip'
with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as z:
    for root, dirs, files in os.walk(src):
        dirs.sort()
        rel = os.path.relpath(root, src)
        arc = 'SD-CARD' if rel == '.' else 'SD-CARD/' + rel.replace(os.sep, '/')
        if not files and not dirs:
            z.write(root, arc + '/')
        for f in sorted(files):
            z.write(os.path.join(root, f), arc + '/' + f)
with zipfile.ZipFile(out) as z:
    bad = z.testzip()
    if bad:
        sys.exit(f'{out}: {bad} is damaged')
    mode = stat.S_IMODE(z.getinfo('SD-CARD/Slot/System/slot').external_attr >> 16)
    if mode & 0o111 == 0:
        sys.exit(f'{out}: Slot/System/slot is not executable')
print(f'wrote {out}')
