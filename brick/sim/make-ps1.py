"""A tiny PlayStation program for the simulator, made from nothing: a PS-EXE that sets up a
320x240 screen, paints four coloured bands and waits, so a scenario can insert, play, snapshot
and eject a real PlayStation game under a real core without shipping anyone's game.
Usage: python3 make-ps1.py OUT.exe"""
import struct
import sys

LOAD = 0x80010000

T0, T1 = 8, 9


def lui(rt, imm):
    return 0x3C000000 | rt << 16 | (imm & 0xFFFF)


def ori(rt, rs, imm):
    return 0x34000000 | rs << 21 | rt << 16 | (imm & 0xFFFF)


def sw(rt, off, base):
    return 0xAC000000 | base << 21 | rt << 16 | (off & 0xFFFF)


GP0, GP1 = 0x1810, 0x1814

program = [lui(T0, 0x1F80)]  # t0 = 0x1F800000, the I/O ports


def put(port, word):
    program.extend([lui(T1, word >> 16), ori(T1, T1, word), sw(T1, port, T0)])


put(GP1, 0x00000000)  # reset the GPU
put(GP1, 0x06C60260)  # horizontal display range, the usual NTSC one
put(GP1, 0x07040010)  # vertical display range: 240 lines
put(GP1, 0x08000001)  # 320x240, NTSC, 15-bit colour
put(GP1, 0x05000000)  # display from VRAM (0, 0)
put(GP0, 0xE3000000)  # drawing area from (0, 0)
put(GP0, 0xE403BD3F)  # ... to (319, 239)
put(GP0, 0xE5000000)  # no drawing offset
# Four bands across the screen, 80 pixels wide each: red, green, blue, white.
for i, colour in enumerate([0x0000FF, 0x00FF00, 0xFF0000, 0xFFFFFF]):
    put(GP0, 0x02000000 | colour)  # fill a rectangle with BGR colour
    put(GP0, i * 80)  # from y 0, x i*80
    put(GP0, 240 << 16 | 80)  # 80 wide, 240 tall
put(GP1, 0x03000000)  # display on
program.extend([0x1000FFFF, 0])  # wait here for ever: b . ; nop

text = b"".join(struct.pack("<I", w) for w in program)
text += b"\0" * (-len(text) % 2048)

header = bytearray(2048)
header[0:8] = b"PS-X EXE"
struct.pack_into("<IIII", header, 0x10, LOAD, 0, LOAD, len(text))  # pc, gp, load address, size
struct.pack_into("<II", header, 0x30, 0x801FFFF0, 0)  # stack
marker = b"Sony Computer Entertainment Inc. for North America area"
header[0x4C:0x4C + len(marker)] = marker

with open(sys.argv[1], "wb") as out:
    out.write(bytes(header) + text)
