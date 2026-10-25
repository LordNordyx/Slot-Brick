"""Sets a FAT32 file system's size: python3 fatsize.py <image> <sectors>.

Writes the sector count into the boot sector and its backup, and marks the free-space count
in both FSInfo sectors unknown. The FAT must already have room for that many sectors
(init's grow does the same on the Brick, with dd).
"""
import struct
import sys

path, sectors = sys.argv[1], int(sys.argv[2])
with open(path, "r+b") as f:
    boot = f.read(512)
    sector_bytes, per_cluster, reserved, fats = struct.unpack_from("<HBHB", boot, 11)
    fat_sectors, fsinfo = struct.unpack_from("<I", boot, 36)[0], struct.unpack_from("<H", boot, 48)[0]
    assert sector_bytes == 512 and fat_sectors, "not a FAT32 with 512-byte sectors"
    most = reserved + fats * fat_sectors + (fat_sectors * 128 - 2) * per_cluster
    clusters = (sectors - reserved - fats * fat_sectors) // per_cluster
    assert sectors <= most, f"the FAT holds {most} sectors at most"
    assert clusters >= 65525, f"{clusters} clusters is too few for FAT32"
    for at in (0, 6 * 512):
        f.seek(at + 32)
        f.write(struct.pack("<I", sectors))
        f.seek(at + fsinfo * 512 + 488)
        f.write(struct.pack("<I", 0xFFFFFFFF))
