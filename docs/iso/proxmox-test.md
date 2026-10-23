# Proxmox test log

Environment: Proxmox VE 9.2, node `pve02`, `local-btrfs` storage, 8 cores, 400+ GiB free.

## What is verified

| Step | Result |
|---|---|
| Ubuntu 24.04.3 live-server ISO (3.3 GB) download | ok |
| Remaster with `xorriso -boot_image any replay` | ok, 3151 MiB, El Torito BIOS + UEFI preserved |
| Seed files embedded (`/user-data`, `/meta-data`, `/nocloud/autoinstall.yaml`) | ok, readable in the ISO |
| Appliance payload embedded (`/nestbox.tar.gz`) | ok |
| GRUB kernel line patched | ok — verified visually: the VM shows the patched menu entry |
| VM boots the remastered ISO | ok |
| Unattended installation | **not yet** — the installer falls back to the interactive language screen |

## The open problem

Two independent signals say the kernel does not receive the autoinstall parameter:

1. the installer starts interactively at language selection instead of showing the autoinstall
   progress screen;
2. `console=ttyS0` on the `linux` line produces no serial output at all, while the same VM writes
   the installer TUI to the VGA console.

The GRUB menu entry itself is definitely ours (a probe build renamed it to `NESTBOX PROBE ENTRY`
and the VM displayed that name), so the file is read. What fails is either argument passing to the
kernel or the NoCloud seed lookup.

Variants tried:

| Kernel arguments | Seed layout | Result |
|---|---|---|
| `autoinstall ds=nocloud;s=/cdrom/nocloud/` | `/nocloud/*` | interactive |
| `autoinstall "ds=nocloud;s=/cdrom/nocloud/"` | `/nocloud/*` | interactive |
| `autoinstall ds=nocloud` | `/user-data`, `/meta-data` at ISO root, volume label `cidata` | interactive |
| the same plus `console=ttyS0` | as above | interactive, no serial output |

## Next steps, in order of likelihood

1. **Use `livefs-editor`** instead of hand-rolled xorriso remastering. It is the tool Ubuntu's own
   documentation uses for exactly this, and it handles the GRUB environment and the seed path.
2. **Check subiquity's datasource at runtime**: boot with `console=ttyS0` once argument passing is
   solved, then read `/var/log/installer/subiquity-server-debug.log` from the live session.
3. **Bypass cloud-init seeding entirely** by baking an `autoinstall.yaml` into the squashfs and
   invoking `subiquity --autoinstall=/autoinstall.yaml` from a custom service.
4. Verify that the semicolon is actually reaching the kernel by appending `ds=nocloud` with an
   explicit `seedfrom=/cdrom/nocloud/` instead of the semicolon form.

## Related finding

A stock Ubuntu 24.04 cloud image does not boot to completion on this host either: it stalls after
`random: crng init done`, never brings the NIC up (no frame on the bridge), and never powers the
guest agent. That is independent of the ISO work and points at the host or the image's kernel, not
at Nestbox. The ISO installer kernel boots normally on the same host.
