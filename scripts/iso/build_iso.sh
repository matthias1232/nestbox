#!/usr/bin/env bash
# Build a Nestbox seed ISO from a stock Ubuntu Server ISO.
#
# The "slim" profile leaves the squashfs untouched. The ISO carries
#   /user-data, /meta-data        the NoCloud seed (volume labelled cidata)
#   /nocloud/*                    the same seed plus the raw autoinstall document
#   /nestbox.tar.gz               the appliance payload
# and a patched GRUB entry that starts the unattended installer.
#
# Two details cost hours to find and are load-bearing:
#   1. subiquity reads the autoinstall document from the seed's user-data under the
#      `autoinstall:` key. A sibling autoinstall.yaml is ignored and the installer silently
#      falls back to the interactive TUI.
#   2. GRUB terminates a command at an unquoted ';', so the datasource is passed by volume
#      label (`ds=nocloud`) rather than as `ds=nocloud;s=/cdrom/nocloud/`.
#
# Requires: xorriso, python3, sha256sum. Runs as root on the build host.
set -euo pipefail

BASE_ISO=""
OUT_ISO=""
SEED_DIR=""
PAYLOAD=""
VOLUME_ID="cidata"

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
  echo
  echo "usage: $0 --base <ubuntu.iso> --out <output.iso> --seed <dir> [--payload <tar.gz>]"
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE_ISO="$2"; shift 2 ;;
    --out) OUT_ISO="$2"; shift 2 ;;
    --seed) SEED_DIR="$2"; shift 2 ;;
    --payload) PAYLOAD="$2"; shift 2 ;;
    --volume-id) VOLUME_ID="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

[ -f "$BASE_ISO" ] || { echo "base ISO not found: $BASE_ISO" >&2; exit 1; }
[ -d "$SEED_DIR" ] || { echo "seed directory not found: $SEED_DIR" >&2; exit 1; }
for f in autoinstall.yaml user-data meta-data; do
  [ -f "$SEED_DIR/$f" ] || { echo "seed file missing: $SEED_DIR/$f" >&2; exit 1; }
done
command -v xorriso >/dev/null || { echo "xorriso is required" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/nocloud"
cp "$SEED_DIR/autoinstall.yaml" "$SEED_DIR/user-data" "$SEED_DIR/meta-data" "$WORK/nocloud/"
# NoCloud accepts a seed at the root of a volume labelled cidata.
cp "$SEED_DIR/user-data" "$SEED_DIR/meta-data" "$WORK/"

echo "[iso] extracting grub.cfg"
xorriso -osirrox on -indev "$BASE_ISO" -extract /boot/grub/grub.cfg "$WORK/grub.cfg" 2>/dev/null

echo "[iso] patching kernel command line"
python3 - "$WORK/grub.cfg" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
marker = "autoinstall ds=nocloud console=ttyS0"
if marker in text:
    print("grub already patched"); sys.exit(0)

def patch(match):
    line = match.group(0)
    if marker in line:
        return line
    if " ---" in line:
        return line.replace(" ---", f" {marker} ---", 1)
    return line + f" {marker}"

patched, count = re.subn(r"^\s*linux\s+.*$", patch, text, count=1, flags=re.MULTILINE)
if count == 0:
    sys.exit("no 'linux' line found in grub.cfg")
open(path, "w", encoding="utf-8").write(patched)
print("grub patched")
PY

if [ -n "$PAYLOAD" ]; then
  [ -f "$PAYLOAD" ] || { echo "payload not found: $PAYLOAD" >&2; exit 1; }
  echo "[iso] embedding payload $(basename "$PAYLOAD")"
  SEED_USER_DATA="$WORK/nocloud/user-data" python3 - <<'PY_U'
import os, yaml
seed = os.environ["SEED_USER_DATA"]
doc = yaml.safe_load(open(seed, encoding="utf-8")) or {}
auto = doc.get("autoinstall") or {}
late = [cmd for cmd in (auto.get("late-commands") or []) if "nestbox" not in str(cmd)]
late += [
    # These run after curtin has mounted the target under /target.  Do not
    # nest `curtin in-target` here: subiquity has already passed each element
    # to curtin, and a second wrapper caused the payload to be skipped on
    # Ubuntu 24.04.
    "mkdir -p /target/opt/nestbox",
    "tar -xzf /cdrom/nestbox.tar.gz -C /target/opt/nestbox --strip-components=1",
    "chmod 0755 /target/opt/nestbox/firstboot/firstboot.sh",
    "install -Dm755 /target/opt/nestbox/firstboot/firstboot.sh /target/usr/local/lib/nestbox/firstboot.sh",
    "install -Dm644 /target/opt/nestbox/firstboot/nestbox-firstboot.service /target/etc/systemd/system/nestbox-firstboot.service",
    "curtin in-target -- systemctl enable nestbox-firstboot.service",
]
auto["late-commands"] = late
doc["autoinstall"] = auto
with open(seed, "w", encoding="utf-8") as fh:
    fh.write("#cloud-config\n")
    yaml.safe_dump(doc, fh, sort_keys=False)
print("payload late-commands injected into user-data")
PY_U
  # keep the standalone copy in sync for reference
  cp "$WORK/nocloud/user-data" "$WORK/nocloud/autoinstall.yaml"
fi

echo "[iso] remastering"
rm -f "$OUT_ISO"
args=(
  -indev "$BASE_ISO" -outdev "$OUT_ISO"
  -map "$WORK/nocloud" /nocloud
  -map "$WORK/user-data" /user-data
  -map "$WORK/meta-data" /meta-data
  -map "$WORK/grub.cfg" /boot/grub/grub.cfg
)
[ -n "$PAYLOAD" ] && args+=(-map "$PAYLOAD" /nestbox.tar.gz)
args+=(-volid "$VOLUME_ID" -boot_image any replay -commit)
xorriso "${args[@]}" 2>&1 | tail -3

echo "[iso] el torito report"
xorriso -indev "$OUT_ISO" -report_el_torito as_mkisofs 2>/dev/null | tail -5 | sed 's/^/  /'

SIZE=$(stat -c%s "$OUT_ISO")
echo "[iso] artifact: $OUT_ISO"
echo "[iso] size: $(( SIZE / 1024 / 1024 )) MiB"
echo "[iso] sha256: $(sha256sum "$OUT_ISO" | cut -d' ' -f1)"
