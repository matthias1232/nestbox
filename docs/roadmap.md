# Roadmap

| Phase | Content | Done when |
|---|---|---|
| 0 — Spike | OpenShell on Ubuntu, headless, real VM (not WSL2) | a script prints green |
| 1 — Foundation | Django, DRF, Swagger/Redoc, auth, inventory, live map, i18n, CI | every container visible and controllable |
| 2 — Sandboxes | SDK adapter, CRUD, templates, exec, logs, ports | create/enter/stop/delete a nest |
| 3 — Policies | structured editors, network rules, global policy, diff, rollback | a rule change affects a running nest |
| 4 — Agent packs | schema, registry, first-party packs, wizard | a chosen agent gets a working URL |
| 5 — Network | traffic container, WireGuard, CoreDNS, ACME, proxy routes | `https://<nest>.apps.<domain>` with a valid cert |
| 6 — ISO | recipe → cloud-init → livefs → first boot → QEMU test | one click produces a bootable ISO |
| 7 — Backup | restic targets, snapshots, single-file restore | restore works from the browser |

Current state: **Phase 1**. The data model and interfaces for later phases exist; the ISO
renderer (recipe → autoinstall + seed) is implemented and tested.
