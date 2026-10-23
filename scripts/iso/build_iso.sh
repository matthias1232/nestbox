#!/usr/bin/env bash
# Build a Nestbox seed ISO from a stock Ubuntu Server ISO.
#
# This is the "slim" profile: the squashfs is untouched, and the ISO carries
#   /{user-data,meta-data}                            the NoCloud seed
#   /nocloud/autoinstall.yaml                         the subiquity document
#   /nestbox.tar.gz                                   the appliance payload
#
# The volume is labelled "cidata" so cloud-init's NoCloud datasource finds the seed by
# label, independent of mount points or GRUB quoting rules.
# plus a patched GRUB entry that starts the unattended installer.
#
# Status: produces a bootable ISO whose GRUB entry is patched correctly, but unattended
# installation does not yet engage on Proxmox VE 9.2 — see docs/iso/proxmox-test.md for the
# evidence and the next steps before relying on this in production.
#
# Requires: xorriso, python3, sha256sum. Runs as root on the build host.
set -euo pipefail

BASE_ISO=""
OUT_ISO=""
SEED_DIR=""
PAYLOAD=""
VOLUME_ID="cidata"

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  echo
  echo "usage: $0 --base <ubuntu.iso> --out <output.iso> --seed <dir> [--payload <tar.gz>] [--volume-id <id>]"
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
# NoCloud also accepts a seed at the filesystem root of a volume labelled cidata.
cp "$SEED_DIR/user-data" "$SEED_DIR/meta-data" "$WORK/"

echo "[iso] extracting grub.cfg"
xorriso -osirrox on -indev "$BASE_ISO" -extract /boot/grub/grub.cfg "$WORK/grub.cfg" 2>/dev/null

echo "[iso] patching kernel command line for autoinstall"
python3 - "$WORK/grub.cfg" <<'PY'
import sys, re
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
# GRUB terminates a command at an unquoted ';', so the datasource string must be
# quoted or the kernel receives only "ds=nocloud" and never finds the seed.
marker = "autoinstall ds=nocloud console=ttyS0"

if marker in text:
    print("grub already patched")
    sys.exit(0)

def patch(match):
    line = match.group(0)
    if marker in line:
        return line
    # Insert the datasource before the trailing '---' separator.
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
  python3 - "$WORK/nocloud/autoinstall.yaml" <<'PY'
import sys, yaml
path = sys.argv[1]
doc = yaml.safe_load(open(path, encoding="utf-8")) or {}
late = list(doc.get("late-commands") or [])
late = [cmd for cmd in late if "nestbox" not in str(cmd)]
late += [
    "cp /cdrom/nestbox.tar.gz /target/opt/nestbox.tar.gz",
    "curtin in-target -- bash -c 'mkdir -p /opt/nestbox && tar -xzf /opt/nestbox.tar.gz -C /opt/nestbox --strip-components=1 && rm -f /opt/nestbox.tar.gz'",
    "curtin in-target -- bash -c 'install -Dm755 /opt/nestbox/firstboot/firstboot.sh /usr/local/lib/nestbox/firstboot.sh'",
    "curtin in-target -- bash -c 'install -Dm644 /opt/nestbox/firstboot/nestbox-firstboot.service /etc/systemd/system/nestbox-firstboot.service'",
    "curtin in-target -- systemctl enable nestbox-firstboot.service",
]
doc["late-commands"] = late
yaml.safe_dump(doc, open(path, "w", encoding="utf-8"), sort_keys=False)
print("late-commands injected")
PY
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
if [ -n "$PAYLOAD" ]; then
  args+=(-map "$PAYLOAD" /nestbox.tar.gz)
fi
args+=(-volid "$VOLUME_ID" -boot_image any replay -commit)
xorriso "${args[@]}" 2>&1 | tail -3

echo "[iso] el torito report"
xorriso -indev "$OUT_ISO" -report_el_torito as_mkisofs 2>/dev/null | tail -6 | sed 's/^/  /'

SIZE=$(stat -c%s "$OUT_ISO")
echo "[iso] artifact: $OUT_ISO"
echo "[iso] size: $(( SIZE / 1024 / 1024 )) MiB"
echo "[iso] sha256: $(sha256sum "$OUT_ISO" | cut -d' ' -f1)"
