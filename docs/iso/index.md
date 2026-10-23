# ISO pipeline

The build turns a recipe into a bootable Ubuntu Server image that is already configured for the
operator who will use it.

## Stages

1. **Discover** — resolve the newest point release of the chosen LTS, verify `SHA256SUMS` and the
   GPG signature. A cached base image is reused when the hash matches.
2. **Render** — the recipe becomes `autoinstall.yaml` (subiquity: identity, network, storage,
   packages, late commands) plus a NoCloud seed (`user-data`, `meta-data`) written to
   `/cdrom/nocloud/`. `autoinstall` is injected into the kernel command line.
3. **Bake** — `livefs-editor` mutates a copy of the ISO: squashfs layers, the OpenShell binary, the
   controller image reference, agent images, the gateway seed, and the driver packages.
4. **First boot** — netplan from the recipe, resolver, TLS material, stack start, gateway
   registration, preseed nest, self-check report.
5. **Verify** — xorriso review, GRUB check, a QEMU boot with a marker string, then SHA256, SBOM and
   provenance.
6. **Publish** — artifact, hash and log are linked in the UI.

## Honest timing

A fully unattended Ubuntu install including squashfs extraction and NVIDIA drivers takes **5–15
minutes**. The target is under ten minutes to the first running nest. "Two minutes" is achievable
only after the installer has finished. Optimisation comes after measurement, not before.

## Secrets

An ISO never contains plaintext credentials. It carries an encrypted envelope and a public key; the
operator unlocks it at first boot, or enters the value in the UI. Afterwards the secret lives in an
OpenShell provider profile, where the agent receives credentials without seeing them.

## Model weights

Never shipped. The Nemotron licence is not Apache-2.0, so weights are pulled at runtime.
