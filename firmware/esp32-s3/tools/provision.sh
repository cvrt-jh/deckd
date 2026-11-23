#!/usr/bin/env bash
# Write the panel's device settings into its NVS partition.
#
# Usage: tools/provision.sh [--dry-run] <serial-port> <panel-name>
#   e.g. tools/provision.sh /dev/cu.usbmodem103401 jh-desk
#
# Reads CVRT_IOT_WIFI_SSID and CVRT_IOT_WIFI_PSK from the environment. When
# either is unset and `with-env` is on PATH, the script re-runs itself under
# `with-env` to load them. Writes namespace `deckpanel` (wifi_ssid, wifi_psk,
# panel_name) with ESP-IDF's nvs_partition_gen.py and flashes it to the nvs
# partition (partitions.csv). --dry-run builds the image and stops before
# flashing. The CSV and the image live in a private temp dir that is removed
# on exit.
#
# The PSK never reaches a trace or a child process: the shell only tests
# whether the variables are set (`[[ -v ]]`, which `bash -x` prints without
# the value), validation and the CSV happen in Python reading the
# environment, and both variables are unset before any other tool runs.
set -euo pipefail

# nvs partition from partitions.csv.
NVS_OFFSET=0x9000
NVS_SIZE=0x6000

usage() {
	echo "usage: $0 [--dry-run] <serial-port> <panel-name>" >&2
	exit 2
}

dry_run=0
if [[ ${1:-} == --dry-run ]]; then
	dry_run=1
	shift
fi
[[ $# -eq 2 ]] || usage
port=$1
name=$2
[[ $name =~ ^[a-z0-9][a-z0-9-]{0,30}$ ]] || {
	echo "provision: panel name must be lowercase letters, digits and dashes (max 31)" >&2
	exit 2
}

if [[ ! -v CVRT_IOT_WIFI_SSID || ! -v CVRT_IOT_WIFI_PSK ]]; then
	if [[ ! -v DECKPANEL_PROVISION_REEXEC ]] && command -v with-env >/dev/null; then
		export DECKPANEL_PROVISION_REEXEC=1
		exec with-env CVRT_IOT_WIFI_SSID CVRT_IOT_WIFI_PSK -- "$0" "$@"
	fi
	echo "provision: CVRT_IOT_WIFI_SSID and CVRT_IOT_WIFI_PSK must be set" >&2
	exit 1
fi

# ESP-IDF checkout and Python env installed by esp-idf-sys
# (ESP_IDF_TOOLS_INSTALL_DIR = "global" -> ~/.espressif).
espressif=${ESPRESSIF_HOME:-$HOME/.espressif}
idf_path=${IDF_PATH:-}
if [[ -z $idf_path ]]; then
	idf_path=$(find "$espressif/esp-idf" -mindepth 1 -maxdepth 1 -type d -name 'v5.*' | sort | tail -n 1)
fi
gen="$idf_path/components/nvs_flash/nvs_partition_generator/nvs_partition_gen.py"
[[ -f $gen ]] || {
	echo "provision: nvs_partition_gen.py not found under '$idf_path' (set IDF_PATH)" >&2
	exit 1
}
python=$(find "$espressif/python_env" -mindepth 3 -maxdepth 3 -path '*/bin/python' | sort | tail -n 1)
[[ -x $python ]] || {
	echo "provision: ESP-IDF python env not found under $espressif/python_env" >&2
	exit 1
}

umask 077
work=$(mktemp -d "${TMPDIR:-/tmp}/deckpanel-nvs.XXXXXX")
trap 'rm -rf "$work"' EXIT

# Validate and write the CSV from the environment. Error messages are fixed
# and never contain a value. csv.writer quotes commas and quotes.
PANEL_NAME=$name "$python" -I -c '
import csv, os, re, string, sys

ssid = os.environ["CVRT_IOT_WIFI_SSID"]
psk = os.environ["CVRT_IOT_WIFI_PSK"]
if not 1 <= len(ssid.encode()) <= 32:
    sys.exit("provision: CVRT_IOT_WIFI_SSID must be 1-32 bytes")
passphrase = 8 <= len(psk) <= 63 and all(c in string.printable[:95] for c in psk)
raw_key = re.fullmatch(r"[0-9a-fA-F]{64}", psk) is not None
if not (passphrase or raw_key):
    sys.exit("provision: CVRT_IOT_WIFI_PSK must be 8-63 printable ASCII characters or 64 hex digits")
with open(sys.argv[1], "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["key", "type", "encoding", "value"])
    w.writerow(["deckpanel", "namespace", "", ""])
    w.writerow(["wifi_ssid", "data", "string", ssid])
    w.writerow(["wifi_psk", "data", "string", psk])
    w.writerow(["panel_name", "data", "string", os.environ["PANEL_NAME"]])
' "$work/nvs.csv"
unset CVRT_IOT_WIFI_PSK CVRT_IOT_WIFI_SSID

"$python" "$gen" generate "$work/nvs.csv" "$work/nvs.bin" "$NVS_SIZE" >/dev/null
rm -f "$work/nvs.csv"
if ((dry_run)); then
	echo "provision: dry run, NVS image for panel '$name' built, not flashed"
	exit 0
fi
espflash write-bin --chip esp32s3 --port "$port" --non-interactive "$NVS_OFFSET" "$work/nvs.bin"
echo "provision: wrote NVS for panel '$name'"
