#!/usr/bin/env bash
# Test: `provision.sh --dry-run` never flashes, also when it loads the
# credentials by re-executing itself under `with-env`.
#
# Runs without ESP tooling: stubs `with-env` (re-execs with fake
# credentials), `espflash` (records that it was called and fails), and an
# ESP-IDF tree whose nvs_partition_gen.py writes an empty image, with the
# system python3 as the ESP-IDF python env.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
python3=$(command -v python3) || {
	echo "test-provision: python3 not found" >&2
	exit 1
}

work=$(mktemp -d "${TMPDIR:-/tmp}/test-provision.XXXXXX")
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/bin" "$work/espressif/esp-idf/v5.5.3/components/nvs_flash/nvs_partition_generator" \
	"$work/espressif/python_env/test_env/bin"

cat >"$work/bin/with-env" <<'EOF'
#!/usr/bin/env bash
# Stub: skip the key names up to `--`, run the rest with fake credentials.
while [[ $# -gt 0 && $1 != -- ]]; do shift; done
shift
exec env CVRT_IOT_WIFI_SSID=testnet CVRT_IOT_WIFI_PSK=testsecret123 "$@"
EOF

cat >"$work/bin/espflash" <<EOF
#!/usr/bin/env bash
echo called >"$work/espflash-called"
echo "stub espflash: must not be called in a dry run" >&2
exit 1
EOF

cat >"$work/espressif/esp-idf/v5.5.3/components/nvs_flash/nvs_partition_generator/nvs_partition_gen.py" <<'EOF'
import sys
# argv: generate <csv> <bin> <size>
open(sys.argv[3], "wb").close()
EOF

ln -s "$python3" "$work/espressif/python_env/test_env/bin/python"
chmod +x "$work/bin/with-env" "$work/bin/espflash"

status=0
env -u CVRT_IOT_WIFI_SSID -u CVRT_IOT_WIFI_PSK -u DECKPANEL_PROVISION_REEXEC -u IDF_PATH \
	PATH="$work/bin:$PATH" ESPRESSIF_HOME="$work/espressif" \
	"$here/provision.sh" --dry-run /dev/null jh-desk >"$work/out.log" 2>&1 || status=$?

fail=0
if [[ $status -ne 0 ]]; then
	echo "FAIL: provision.sh --dry-run exited $status" >&2
	fail=1
fi
if [[ -e $work/espflash-called ]]; then
	echo "FAIL: espflash was called in a dry run" >&2
	fail=1
fi
if grep -q testsecret123 "$work/out.log"; then
	echo "FAIL: the PSK appeared in the output" >&2
	fail=1
fi
if [[ $fail -ne 0 ]]; then
	sed 's/^/  | /' "$work/out.log" >&2
	exit 1
fi
echo "ok: provision.sh --dry-run under with-env re-exec did not flash"
