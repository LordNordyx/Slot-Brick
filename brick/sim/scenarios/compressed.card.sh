# The compressed scenario's card: Pocket Test zipped, 7z'd and in a damaged zip, on the GB
# shelf beside the plain one.
set -eu
games="$1/Games/GB"
rom="$games/Pocket Test.gb"
python3 -I brick/sim/make-archive.py zip "$rom" "$games/Pocket Zip.zip"
python3 -I brick/sim/make-archive.py 7z "$rom" "$games/Pocket 7z.7z"
python3 -I brick/sim/make-archive.py broken-zip "$rom" "$games/Pocket Broken.zip"
