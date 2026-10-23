# Proxmox and QEMU test log

This page records what has actually been executed on hardware, so a reader can tell a verified
statement from an expectation.

## Environment

| Item | Value |
|---|---|
| Proxmox VE | 9.2.21, node `pve02`, `local-btrfs` for disks, `local` for ISO images |
| QEMU | 11.0.3 (the same binary Proxmox uses) |
| Test guest | 4 GB RAM, 2 vCPU, 60 GB disk, VirtIO NIC on a user-mode network with DHCP |
| Base image | `ubuntu-24.04-live-server-amd64.iso` |
| Nestbox image | `nestbox-24.04-slim.iso`, 3151 MiB, SHA-256 `34c76c4f516526f6c3e5733997c2264892e20e1fedfd1db7c9200ca9beb98db7` |

## What is verified

| Step | Result |
|---|---|
| Remaster with `xorriso -boot_image any replay` | ok — El Torito BIOS and UEFI entries preserved |
| Seed embedded (`/user-data`, `/meta-data`, `/nocloud/*`, volume label `cidata`) | ok |
| Payload embedded (`/nestbox.tar.gz`, 2.3 MB) | ok |
| GRUB `linux` line patched with `autoinstall ds=nocloud` | ok |
| Unattended install, no question asked | ok — reboot after 236 s |
| Installed system boots, `cloud-final` finishes without failure | ok |
| `nestbox-firstboot.service` present and enabled by the installer | ok |
| First boot installs Docker, writes `/etc/nestbox/nestbox.env`, creates the certificate | ok |
| Controller image builds inside the guest and starts | ok — PostgreSQL healthy, controller up |
| UI answers on the configured port (443) with the self-signed certificate | ok — `HTTP 302` to `/accounts/login/` |
| Sign-in with the generated administrator account | ok |
| Dashboard reads the Docker socket | ok — both containers listed as running |

The same sequence was repeated on Proxmox VE with a VM created through the web UI (VM 210, 4 GB,
2 cores, 60 GB on `local-btrfs`, ISO attached as `ide2`), which is the walkthrough in the
[install guide](https://nestbox.mmbits.com/guide/).

## The four load-bearing details

Each of these cost a failed install cycle. They are the reason the build script looks the way it does.

1. **The autoinstall document must be nested in the seed's `user-data`.** A sibling
   `autoinstall.yaml` on the ISO is ignored and subiquity silently falls back to the interactive
   installer. The document therefore lives under `autoinstall:` inside `/user-data`.
2. **GRUB ends a command at an unquoted `;`.** The datasource is passed as `ds=nocloud` and found
   through the `cidata` volume label instead of `ds=nocloud;s=/cdrom/nocloud/`.
3. **The payload's late-commands belong in `user-data` as well**, not in `autoinstall.yaml`. With
   them in the wrong file the payload was copied but `nestbox-firstboot.service` was never enabled.
4. **The installer must not need a package mirror.** Only `openssh-server` comes from the ISO pool;
   everything else is installed on first boot, with retries.

All late-commands are written as `… || true`: one failing command must never abort an otherwise
good install.

## Runtime defects this test found

These only appear after the install, when the controller starts for the first time:

| Symptom | Cause | Fix |
|---|---|---|
| `collectstatic` fails: `KeyError: 'NESTBOX_SECRET_KEY'` | the production settings require the runtime secret at image build time | the build layer passes a throwaway key |
| `ModuleNotFoundError: unfold` | the admin theme was used but never declared | `django-unfold` is a dependency |
| `ModuleNotFoundError: fido2` | allauth's MFA URLs are mounted, but the extra was missing | `django-allauth[mfa]` |
| gunicorn: `PermissionError` on `key.pem` | the container runs as uid 10001, the key was `0600 root` | first boot hands the pair to 10001 |
| dashboard: `runtime.unavailable` | uid 10001 is not in the Docker socket's group | first boot records the host's Docker GID, compose adds the group |
| unstyled sign-in page | allauth's default templates | branded `account/login.html` and `account/logout.html` |

## Reproduce it without Proxmox

Any Linux host with KVM, `xorriso` and `qemu-img` can run the same test:

```sh
./scripts/iso/build_iso.sh \
  --base ubuntu-24.04-live-server-amd64.iso \
  --out  dist/nestbox-24.04-slim.iso \
  --seed dist/seed \
  --payload dist/nestbox.tar.gz

qemu-img create -f qcow2 /tmp/nestbox.qcow2 60G
qemu-system-x86_64 -m 4096 -smp 2 -cpu host -enable-kvm \
  -drive file=/tmp/nestbox.qcow2,if=virtio,format=qcow2 \
  -cdrom dist/nestbox-24.04-slim.iso \
  -display none -serial file:/tmp/install.log -no-reboot
```

The install is done when `/tmp/install.log` ends with `reboot: Restarting system`. Boot the disk
again, and follow `/var/log/nestbox/firstboot.log` until it prints `[nestbox] first boot complete`
together with the URL and the generated administrator password.

## Not covered yet

- Real hardware, and Proxmox with a physical NIC trunk or VLAN tags.
- Ubuntu 22.04 and 26.04 base images (the seed format is the same, the ISOs are untested).
- ACME certificates; the appliance ships with a self-signed pair and the TLS app has a certbot
  wrapper, but no public domain has been exercised end to end.
