#!/usr/bin/env bash
# Flash the firmware ELF with the bootloader esp-idf-sys built from
# sdkconfig.defaults and with partitions.csv, then open the monitor.
# `cargo run` uses this as its runner (.cargo/config.toml).
#
# Usage: tools/flash.sh <elf> [extra espflash flash args...]
#   Port: espflash's ESPFLASH_PORT env var, or it asks.
#
# The bootloader sits in esp-idf-sys' hashed build dir next to the ELF, so a
# static runner line cannot name it; espflash's bundled bootloader would boot
# in DIO at 40 MHz instead of our QIO settings.
set -euo pipefail

[[ $# -ge 1 ]] || {
	echo "usage: $0 <elf> [espflash flash args...]" >&2
	exit 2
}
elf=$1
shift
here=$(cd "$(dirname "$0")/.." && pwd)
release_dir=$(dirname "$elf")

shopt -s nullglob
boots=("$release_dir"/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin)
shopt -u nullglob
((${#boots[@]} > 0)) || {
	echo "flash: no bootloader.bin under $release_dir/build/esp-idf-sys-*/ (build first)" >&2
	exit 1
}
# Newest build wins when several esp-idf-sys build dirs exist.
boot=${boots[0]}
for candidate in "${boots[@]}"; do
	[[ $candidate -nt $boot ]] && boot=$candidate
done

exec espflash flash --monitor --chip esp32s3 \
	--bootloader "$boot" \
	--partition-table "$here/partitions.csv" \
	--flash-size 16mb \
	"$@" "$elf"
