#!/bin/sh
# Capture the README screenshot and demo GIF of the numr TUI.
#
# Hyprland-only (tested on omarchy, 2x display). Needs: alacritty, grim, wf-recorder,
# ffmpeg, magick, jq, ydotool with ydotoold running as root (sudo ydotoold &), and a
# release build: cargo build --release -p numr-tui.
#
#   scripts/capture-media.sh     # writes screenshots/finances.png and screenshots/typing.gif
#
# Runs the TUI in a floating, opaque Alacritty at a fixed size, strips compositor
# rounding/border, opens docs/media/screenshot.numr for the PNG, then types docs/media/demo.numr into an
# empty document (Vim insert mode) while wf-recorder records. Keyboard layouts and
# Hyprland settings are restored at the end. Keep the pointer away from other windows.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
bin=$root/target/release/numr
out=$root/screenshots
work=${TMPDIR:-/tmp}/numr-capture.$$
mkdir -p "$work" "$out"
[ -x "$bin" ] || { echo "build first: cargo build --release -p numr-tui" >&2; exit 1; }

class=numr-capture
win_x=100 win_y=100 png_w=960 png_h=600 gif_w=960 gif_h=460
# ydotool absolute coordinates are logical pixels / 2 on this display.
ydo() { sudo ydotool "$@"; }
at() { ydo mousemove -a -x $(( ($1) / 2 )) -y $(( ($2) / 2 )); sleep 0.25; }
win_addr() { hyprctl clients -j | jq -r ".[] | select(.class==\"$class\") | .address"; }
dsp() { hyprctl dispatch "$1" > /dev/null; }

cleanup() {
	pkill -INT wf-recorder 2>/dev/null || true
	pkill -f "alacritty --class $class" 2>/dev/null || true
	hyprctl switchxkblayout ydotoold-virtual-device 1 > /dev/null 2>&1 || true
	hyprctl switchxkblayout keyd-virtual-keyboard 1 > /dev/null 2>&1 || true
	hyprctl reload > /dev/null 2>&1 || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM

# Opaque terminal at a readable size; the omarchy config has opacity 0.94 and a 9pt font.
term() { # file
	alacritty --class "$class" -T numr -o window.opacity=1.0 -o font.size=12 \
		-o window.padding.x=16 -o window.padding.y=12 -e "$bin" "$1" > "$work/alacritty.log" 2>&1 &
	until [ -n "$(win_addr)" ]; do sleep 0.5; done
	sleep 1.5
	addr=$(win_addr)
}

# Hyprland rounds every window and draws a border; both come back after re-floating,
# so strip them each time the window is placed, then prove it by sampling the corners.
undecorate() {
	for prop in "rounding 0" "border_size 0" "opaque 1" "no_blur 1" "no_shadow 1" "no_dim 1"; do
		dsp "hl.dsp.window.set_prop({ window = \"address:$addr\", prop = \"${prop% *}\", value = ${prop#* } })"
	done
	hyprctl keyword general:border_size 0 > /dev/null
	sleep 0.5
}
verify_corners() {
	grim -g "$win_x,$win_y 24x24" "$work/corner.png"
	c0=$(magick "$work/corner.png" -format '%[pixel:p{0,0}]' info:)
	c3=$(magick "$work/corner.png" -format '%[pixel:p{3,3}]' info:)
	c8=$(magick "$work/corner.png" -format '%[pixel:p{8,8}]' info:)
	[ "$c0" = "$c8" ] && [ "$c3" = "$c8" ] && return 0
	echo "corner is decorated: $c0 / $c3 vs $c8 (rounding or border still on)" >&2
	exit 1
}
place() { # width height
	for _ in 1 2 3 4 5; do
		floating=$(hyprctl clients -j | jq -r ".[] | select(.address==\"$addr\") | .floating")
		[ "$floating" = true ] || dsp "hl.dsp.window.float({ window = \"address:$addr\", action = \"toggle\" })"
		sleep 0.5
		dsp "hl.dsp.window.resize({ window = \"address:$addr\", x = $1, y = $2 })"
		sleep 0.8
		dsp "hl.dsp.window.move({ window = \"address:$addr\", x = $win_x, y = $win_y })"
		sleep 0.6
		pos=$(hyprctl clients -j | jq -r ".[] | select(.address==\"$addr\") | \"\\(.at[0]),\\(.at[1]) \\(.size[0])x\\(.size[1])\"")
		[ "$pos" = "$win_x,$win_y $1x$2" ] && { undecorate; return 0; }
	done
	echo "window did not settle at $win_x,$win_y $1x$2 (got $pos)" >&2
	exit 1
}
focus() {
	dsp "hl.dsp.focus({ window = \"address:$addr\" })"; sleep 0.4
	[ "$(hyprctl activewindow -j | jq -r .class)" = "$class" ] || { echo "terminal is not focused; keep the pointer away from other windows" >&2; exit 1; }
}

hyprctl keyword input:accel_profile flat > /dev/null
hyprctl keyword cursor:hide_on_key_press true > /dev/null
hyprctl switchxkblayout keyd-virtual-keyboard 0 > /dev/null
hyprctl switchxkblayout ydotoold-virtual-device 0 > /dev/null

# 1. Screenshot: the sample finance document.
term "$root/docs/media/screenshot.numr"
place $png_w $png_h
at $((win_x + png_w - 40)) $((win_y + png_h - 30))   # pointer parks inside the window, bottom right
focus
ydo key 42:1 42:0; sleep 1.5                        # Shift tap hides the pointer
verify_corners
grim -g "$win_x,$win_y ${png_w}x${png_h}" "$out/finances.png"
pkill -f "alacritty --class $class"; sleep 1

# 2. GIF: empty document, Vim insert mode, live typing, help popup, Standard mode.
term "$work/demo.numr"
place $gif_w $gif_h
at $((win_x + gif_w - 40)) $((win_y + gif_h - 30))
focus
sleep 1
verify_corners
wf-recorder -g "$win_x,$win_y ${gif_w}x${gif_h}" -f "$work/rec.mp4" -r 30 -c libx264rgb -p crf=8 > "$work/wf.log" 2>&1 &
sleep 1.2
focus
ydo type -d 60 -H 20 "i"; sleep 0.6                 # enter insert mode
ydo type -d 28 -H 14 -f "$root/docs/media/demo.numr"; sleep 1.0
ydo key 1:1 1:0; sleep 1.2                          # Esc back to Normal mode
ydo type -d 60 -H 20 "?"; sleep 2.4                 # help popup
ydo key 1:1 1:0; sleep 1.0                          # Esc closes it
ydo key 42:1 15:1 15:0 42:0; sleep 1.2              # Shift+Tab: Standard mode, direct typing
ydo key 28:1 28:0; ydo type -d 28 -H 14 "2.5 hours to minutes"; sleep 1.6
ydo key 42:1 15:1 15:0 42:0; sleep 2.0              # Shift+Tab: back to Vim
pkill -INT wf-recorder; sleep 2

ffmpeg -v error -y -i "$work/rec.mp4" \
	-vf "fps=15,scale=${gif_w}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192:stats_mode=diff[p];[s1][p]paletteuse=dither=none:diff_mode=rectangle" \
	"$out/typing.gif"
ls -la "$out"
