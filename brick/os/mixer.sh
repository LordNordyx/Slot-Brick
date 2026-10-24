#!/bin/sh
# The Brick's mixer at startup, as TrimUI's stock system leaves it (their firmware 1.1.1:
# /etc/rc.local, then runtrimui.sh, whose `tinymix set 9 1` and `tinymix set 1 0` are
# Headphone Volume and DAC Swap). The codec comes up with its outputs switched off; the speaker
# is fed from the headphone output, through its amplifier, and the line out is unused.
# Slot sets the volume itself, in software, so these stay fixed.
set_control() { amixer -q cset name="$1" "$2" || echo "mixer: $1 = $2 failed"; }
set_control 'Headphone Switch' on
set_control 'HpSpeaker Switch' on        # the speaker amplifier
set_control 'Headphone Volume' 1         # of 7
set_control 'DAC Swap' Off
