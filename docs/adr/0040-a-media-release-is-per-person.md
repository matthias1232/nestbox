# ADR 0040 — A media release is per person, not only per environment

Status: Accepted
Date: 2026-10-30

## Context

ADR 0039 gave the environment one allow-list of providers and let any operator
narrow their own sandbox within it. That answers "what may this appliance offer?"
but not "what may *this person* use?": two operators in one environment often
need different image budgets, and a viewer may need to see one provider while
another is being trialled.

## Decision

1. **A grant row is per person and environment** (`user_media_grants`):
   `provider_ids` and `can_configure`.
2. **Three states, deliberately distinct.**
   - no row → inherit the environment allow-list, may configure;
   - empty list → everything the environment allows (same effect, written down);
   - a non-empty list → only those, intersected with the environment allow-list.
3. **`can_configure = false` is a read-only release.** The person sees the
   models in the console but a request to change their sandbox's choice is a 403
   naming the reason, even though their role would otherwise allow `sandbox.write`.
4. **An admin is never restricted.** The admin writes the lists; a list that
   binds the admin would be a way to lock the appliance out of its own setting.
5. **The release follows the sandbox into rendering.** The bridge checks the
   *owner's* grant, not the caller's: an agent cannot render a model its owner
   was not released. A shared sandbox (no owner) is bounded by the environment
   allow-list alone.
6. **A no-op row is deleted.** Granting everything back removes the row rather
   than storing "empty and allowed", so a later change of the default reaches the
   person again.
7. **Every change is audited** (`media.grant`, subject `user`), because a release
   is a permission decision, not a preference.

## Consequences

* The console's Media page gains an admin-only "User releases" table; a
  non-admin's provider list is already filtered by their release, so the same
  page shows them only what they may use.
* A person with an empty release sees no providers and cannot choose one; the
  message names the reason rather than showing an empty dropdown.
* The model still lives per sandbox (`sandbox_media_settings`); the grant narrows
  which of them a person may pick. The two are checked together on every write.

## Not decided here

* Releases are per provider, not per kind: someone released for video is not
  automatically released for image. A per-kind release would need three lists;
  the shape here allows it later without a migration of the decision.
