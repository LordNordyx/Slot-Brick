#!/bin/sh
# The Brick's mixer at startup: the codec comes up with its outputs switched off. Slot sets
# the volume itself, in software, so these stay fixed.
# The levels are, for now, those Knulli uses for the Brick; to be checked against stock's own
# (Slot's diagnose.sh on stock records them, see "mixer" in diagnostics.txt).
set_control() { amixer -q cset name="$1" "$2" || echo "mixer: $1 = $2 failed"; }
set_control 'LINEOUT Switch' on          # the speaker
set_control 'HpSpeaker Switch' on        # the speaker amplifier
set_control 'Headphone Switch' on
set_control 'LINEOUT Output Select' 0    # DAC_SINGLE
set_control 'LINEOUT volume' 26          # of 31
set_control 'Headphone Volume' 2         # of 7
set_control 'DAC volume' 0,0
set_control 'digital volume' 0
set_control 'DAC Swap' 1
