"""A stand-in release for the simulator to update to: a card zip holding a pretend Slot, and the
update.txt describing it, in a folder laid out as GitHub's releases are. brick/sim.sh signs
update.txt with the simulator's key; SLOT_UPDATE_URL points the Update screen at the folder.
Usage: python3 make-update.py RELEASES_DIR"""
import hashlib
import os
import sys
import zipfile

root = sys.argv[1]
os.makedirs(os.path.join(root, "latest/download"), exist_ok=True)
os.makedirs(os.path.join(root, "download/v9.9"), exist_ok=True)
zip_path = os.path.join(root, "download/v9.9/slot-brick-v9.9-sd-card.zip")
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
    for name, text in [("Slot/System/slot", "#!/bin/sh\nexit 0\n"), ("Slot/System/launch.sh", "exit 0\n")]:
        info = zipfile.ZipInfo("SD-CARD/" + name, (2026, 1, 1, 0, 0, 0))
        info.external_attr = 0o100755 << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, text)
data = open(zip_path, "rb").read()
notes = [
    "- Something new on the shelf, with a line long enough that it has to be wrapped onto a second one.",
    "- A fix for something that was broken.",
]
manifest = "\n".join([
    "slot-update 1",
    "channel=release",
    "version=v9.9",
    "commit=0123456789abcdef0123456789abcdef01234567",
    "zip=slot-brick-v9.9-sd-card.zip",
    f"size={len(data)}",
    f"sha256={hashlib.sha256(data).hexdigest()}",
    "",
    *notes,
]) + "\n"
with open(os.path.join(root, "latest/download/update.txt"), "w") as f:
    f.write(manifest)
