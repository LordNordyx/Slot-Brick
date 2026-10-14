#!/usr/bin/env python3
"""Regenerate card/Slot/System/{nes,famicom,snes}-cart-labels.txt from the LaunchBox Games Database.

The slot art set behind Cart Studio has no NES or SNES label scans, so Cart Art takes those
labels from LaunchBox's "Cart - Front" pictures: whole-cart photos it crops the label out of
(cart_art.rs). This keeps the ones of the console's standard US cart (PNG cut-outs, from the
regions that cart was sold in; Famicom, Super Famicom and PAL SNES carts are other shapes)
with the game's name and region, so the Brick never downloads the 100 MB metadata itself.
"famicom" keeps the NES games' Japanese pictures instead: the Famicom cart.

The download is untrusted input: it is parsed with the standard library's expat (2.4+
guards against entity expansion), and only names without control characters and file names
of plain characters make it into the list, which the Brick checks again before any request.

Usage: python3 brick/make-cart-labels.py nes|famicom|snes [Metadata.zip]
"""
import pathlib, sys, tempfile, urllib.request, zipfile
import xml.etree.ElementTree as ET

URL = 'https://gamesdb.launchbox-app.com/Metadata.zip'
# LaunchBox regions as the No-Intro tags slot's name matching scores. The NES cart was the
# same in the US and Europe; the SNES cart of Europe (and Japan) is another shape.
AMERICAS = {'North America': 'USA', 'United States': 'USA', 'Canada': 'Canada', 'World': 'World', '': ''}
EUROPE = {'Europe': 'Europe', 'United Kingdom': 'UK', 'Germany': 'Germany', 'France': 'France',
          'Spain': 'Spain', 'Italy': 'Italy', 'Australia': 'Australia', 'Sweden': 'Sweden',
          'Netherlands': 'Netherlands'}
CONSOLES = {
    'nes': ('Nintendo Entertainment System', {**AMERICAS, **EUROPE}),
    'famicom': ('Nintendo Entertainment System', {'Japan': 'Japan'}),
    'snes': ('Super Nintendo Entertainment System', AMERICAS),
}
if len(sys.argv) < 2 or sys.argv[1] not in CONSOLES:
    sys.exit(__doc__)
PLATFORM, REGIONS = CONSOLES[sys.argv[1]]
OUT = pathlib.Path(__file__).parent / f'card/Slot/System/{sys.argv[1]}-cart-labels.txt'
SAFE = set('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.')


def metadata():
    if len(sys.argv) > 2:
        return zipfile.ZipFile(sys.argv[2])
    tmp = tempfile.NamedTemporaryFile(suffix='.zip')
    request = urllib.request.Request(URL, headers={'User-Agent': 'slot-brick make-cart-labels'})
    with urllib.request.urlopen(request, timeout=600) as r:
        while chunk := r.read(1 << 20):
            tmp.write(chunk)
    tmp.flush()
    return zipfile.ZipFile(tmp.name)


games, names, carts = {}, {}, {}
with metadata().open('Metadata.xml') as f:
    for _, el in ET.iterparse(f):
        if el.tag == 'Game':
            if el.findtext('Platform') == PLATFORM:
                games[el.findtext('DatabaseID')] = el.findtext('Name')
            el.clear()
        elif el.tag == 'GameAlternateName':
            names.setdefault(el.findtext('DatabaseID'), []).append(
                (el.findtext('AlternateName') or '', el.findtext('Region') or ''))
            el.clear()
        elif el.tag == 'GameImage':
            if el.findtext('Type') == 'Cart - Front':
                carts.setdefault(el.findtext('DatabaseID'), []).append(
                    (el.findtext('FileName') or '', el.findtext('Region') or ''))
            el.clear()

lines = set()
for gid, name in games.items():
    for file, region in carts.get(gid, []):
        if region not in REGIONS or not file.endswith('.png') or not set(file) <= SAFE or len(file) > 64:
            continue
        tag = REGIONS[region]
        for title, alt_region in [(name, '')] + names.get(gid, []):
            # The game's main (English) name too, so a ROM named for its US release finds its
            # Famicom picture.
            if (alt_region in REGIONS or not alt_region) and title and not any(ord(c) < 32 for c in title):
                lines.add(f'{title} ({tag})\t{file}' if tag else f'{title}\t{file}')
header = f'# {PLATFORM} cart pictures from the LaunchBox Games Database ({URL}): name (region), file\n'
OUT.write_text(header + '\n'.join(sorted(lines)) + '\n')
print(f'{len(lines)} names for {sum(1 for g in games if g in carts)} games; wrote {OUT}', file=sys.stderr)
