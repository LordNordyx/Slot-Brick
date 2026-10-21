"""A tiny PlayStation program for the simulator, made from nothing: a PS-EXE that sets up a
320x240 screen, paints four coloured bands and waits, so a scenario can insert, play, snapshot
and eject a real PlayStation game under a real core without shipping anyone's game.
Usage: python3 make-ps1.py OUT.exe | OUT.iso [DISC]
OUT.iso puts the program on a disc image of its own (ISO 9660, booted by its SYSTEM.CNF), so
the discs of a game can be told apart: DISC (from 1) turns the bands round that many places."""
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
# Four bands across the screen, 80 pixels wide each: red, green, blue, white, turned round once
# for each disc after the first.
disc = int(sys.argv[2]) if len(sys.argv) > 2 else 1
bands = [0x0000FF, 0x00FF00, 0xFF0000, 0xFFFFFF]
bands = bands[disc - 1:] + bands[:disc - 1]
for i, colour in enumerate(bands):
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

exe = bytes(header) + text

SECTOR = 2048


def both32(v):
    return struct.pack("<I", v) + struct.pack(">I", v)


def both16(v):
    return struct.pack("<H", v) + struct.pack(">H", v)


def record(name, sector, size, directory):
    """An ISO 9660 directory record."""
    body = both32(sector) + both32(size) + bytes(7) + bytes([2 if directory else 0, 0, 0]) + both16(1)
    body += bytes([len(name)]) + name
    if len(name) % 2 == 0:
        body += b"\0"
    return bytes([len(body) + 2, 0]) + body


def iso(exe):
    cnf = b"BOOT = cdrom:\\PSX.EXE;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFFF0\r\n"
    root, cnf_at, exe_at = 20, 21, 22
    total = exe_at + -(-len(exe) // SECTOR)
    dirents = record(b"\0", root, SECTOR, True) + record(b"\1", root, SECTOR, True)
    dirents += record(b"PSX.EXE;1", exe_at, len(exe), False) + record(b"SYSTEM.CNF;1", cnf_at, len(cnf), False)
    pvd = bytearray(SECTOR)
    pvd[0:7] = b"\1CD001\1"
    pvd[8:40] = b"PLAYSTATION".ljust(32)
    pvd[40:72] = b"SLOTSIM".ljust(32)
    pvd[80:88] = both32(total)
    pvd[120:124] = both16(1)
    pvd[124:128] = both16(1)
    pvd[128:132] = both16(SECTOR)
    pvd[132:140] = both32(10)
    pvd[140:144] = struct.pack("<I", 18)
    pvd[148:152] = struct.pack(">I", 19)
    pvd[156:190] = record(b"\0", root, SECTOR, True)
    pvd[881] = 1
    end = bytearray(SECTOR)
    end[0:7] = b"\xffCD001\1"
    path_l = bytes([1, 0]) + struct.pack("<I", root) + struct.pack("<H", 1) + b"\0\0"
    path_m = bytes([1, 0]) + struct.pack(">I", root) + struct.pack(">H", 1) + b"\0\0"
    sectors = [bytes(SECTOR)] * 16 + [bytes(pvd), bytes(end), path_l, path_m, dirents, cnf, exe]
    return b"".join(s + bytes(-len(s) % SECTOR) for s in sectors)


out = sys.argv[1]
with open(out, "wb") as f:
    f.write(iso(exe) if out.lower().endswith(".iso") else exe)
