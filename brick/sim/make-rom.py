"""A tiny Game Boy ROM for the simulator, made from nothing: it shows a still four-shade
pattern and waits, so a scenario can insert, play, snapshot and eject a real cart under a real
core without shipping anyone's game. Usage: python3 make-rom.py OUT.gb"""
import sys

rom = bytearray(0x8000)

# Entry: jump past the header to the program.
rom[0x100:0x104] = bytes([0x00, 0xC3, 0x50, 0x01])  # nop; jp 0x0150
# mGBA recognises a Game Boy ROM by the first four bytes of the header's logo; with the boot
# ROM skipped, as slot runs it, nothing reads the rest.
rom[0x104:0x108] = bytes([0xCE, 0xED, 0x66, 0x66])
rom[0x134:0x13C] = b"SLOTSIM\0"  # title
rom[0x147] = 0x00  # ROM only
rom[0x148] = 0x00  # 32 KiB
rom[0x149] = 0x00  # no RAM
rom[0x14A] = 0x01  # not Japan

TILES = 0x200
MAP = 0x300
program = bytes([
    0xF3,                    # di
    0x31, 0xFE, 0xFF,        # ld sp, 0xFFFE
    0xF0, 0x44,              # wait: ldh a, (LY)
    0xFE, 0x90,              #   cp 144
    0x20, 0xFA,              #   jr nz, wait
    0xAF,                    # xor a
    0xE0, 0x40,              # ldh (LCDC), a      screen off while VRAM is filled
    0x21, 0x00, 0x80,        # ld hl, 0x8000      tiles
    0x11, TILES & 0xFF, TILES >> 8,
    0x01, 0x40, 0x00,        # ld bc, 64
    0x1A, 0x22, 0x13, 0x0B, 0x78, 0xB1, 0x20, 0xF8,  # copy: ld a,(de); ld (hl+),a; inc de; dec bc; ld a,b; or c; jr nz
    0x21, 0x00, 0x98,        # ld hl, 0x9800      background map
    0x11, MAP & 0xFF, MAP >> 8,
    0x01, 0x00, 0x04,        # ld bc, 1024
    0x1A, 0x22, 0x13, 0x0B, 0x78, 0xB1, 0x20, 0xF8,  # copy
    0x3E, 0xE4,              # ld a, 0xE4
    0xE0, 0x47,              # ldh (BGP), a
    0x3E, 0x91,              # ld a, 0x91
    0xE0, 0x40,              # ldh (LCDC), a      screen on, background on
    0x18, 0xFE,              # jr $               and wait
])
rom[0x150:0x150 + len(program)] = program

# Four tiles, one per shade: 2 bits a pixel, as (low, high) byte pairs per row.
for tile, (low, high) in enumerate([(0x00, 0x00), (0xFF, 0x00), (0x00, 0xFF), (0xFF, 0xFF)]):
    rom[TILES + tile * 16:TILES + tile * 16 + 16] = bytes([low, high] * 8)
# Diagonal bands four tiles wide.
for y in range(32):
    for x in range(32):
        rom[MAP + y * 32 + x] = ((x // 4) + (y // 4)) % 4

rom[0x14D] = (-sum(rom[0x134:0x14D]) - (0x14D - 0x134)) & 0xFF  # header checksum
total = sum(rom) - rom[0x14E] - rom[0x14F]
rom[0x14E:0x150] = (total & 0xFFFF).to_bytes(2, "big")  # global checksum

with open(sys.argv[1], "wb") as f:
    f.write(rom)
