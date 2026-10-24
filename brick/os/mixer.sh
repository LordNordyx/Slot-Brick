#!/bin/sh
# The Brick's mixer at startup, as TrimUI's stock system sets it (/etc/rc.local and
# runtrimui.sh in their firmware 1.1.1): the codec comes up with its outputs switched off.
# The speaker is fed from the headphone output, through its amplifier; the line out is unused.
# Slot sets the volume itself, in software, so these stay fixed.
set_control() { amixer -q cset "$1" "$2" || echo "mixer: $1 = $2 failed"; }
set_control name='Headphone Switch' 1
set_control name='Headphone Volume' 3     # of 7
set_control name='HpSpeaker Switch' 1     # the speaker amplifier
# runtrimui.sh's `tinymix set 9 1` and `tinymix set 1 0`: tinymix counts controls from 0,
# ALSA's numid from 1.
set_control numid=10 1
set_control numid=2 0
