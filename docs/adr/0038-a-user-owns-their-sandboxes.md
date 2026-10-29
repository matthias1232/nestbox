# ADR 0038 — A user owns their sandboxes; an admin sees and sets everything shared

Status: Accepted
Date: 2026-10-29

## Context

Every sandbox of an environment was visible and writable by every member. The roles
(admin, operator, viewer) decided *what kind* of action was allowed, never *on whose*
sandbox. For a product that several people use, and where each should run their own
agents from start to finish, that is not enough, and there was no way to create a user.

## Decision

1. **A sandbox has an owner**, the user who created it (`sandboxes.owner_user_id`).
2. **Row-level security decides who sees it**, in the database, so no endpoint can forget
   the check: an admin sees every sandbox; anyone else sees their own and the shared ones
   (no owner); the control plane itself, acting for nobody, sees all. The acting user and
   role are set per transaction next to the environment (`TenantScope::begin_for`).
3. **What belongs to a sandbox follows it.** Persistent roots and their backups, telemetry
   rows, sandbox-specific network rules and supervisors get the same owner-or-admin policy.
   The live streams drop events of sandboxes the caller may not see. Endpoints that read by
   sandbox id load the sandbox first, so another user's id is a plain not-found.
4. **What is shared is set by admins.** Providers and the keys behind them, gateways, global
   policy, the audit trail and user administration are admin-only; users choose from what the
   admin made available.
5. **Users are created by an admin** (name, email, role, a password of at least 12
   characters). Guards: an admin cannot demote, disable or remove themselves, the last admin
   stays, a role change or a disabled account ends the user's sessions at once, and removing a
   user leaves their sandboxes shared rather than orphaned.
6. Sandboxes that existed before ownership went to the first admin of their organization, so
   adding a user does not expose them.

## Not decided here

* **Several environments as tenants** (an environment per team, onboarding it with a cluster,
  gateway and node) are not built: sandbox names are unique per environment but are container
  and OpenShell names on one host, so they would have to be namespaced first.
* **Policies are shared documents.** An operator may edit a policy that other users' sandboxes
  carry; a per-owner policy scope is the next step.
* Telemetry rows about *no* sandbox (gateway, node) are visible to admins only.
