// Persistent roots: directories on a node, and the mounts that bind them into
// sandboxes.
//
// The host path is derived by the server from kind, owner, and slug. A create
// body therefore has no path field; a caller who wants one is asking for
// arbitrary host access and the API refuses to offer it.

import { request, query } from './client.js';

/** Every root that is not archived, with where it is mounted. */
export function list() {
  return request('persistent-roots');
}

/** One root with its mounts. */
export function get(id) {
  return request(`persistent-roots/${id}`);
}

/**
 * Create a root. `kind` is `agent_home` or `project`; `node` is a node name and
 * omitted means the local node. The returned row carries the derived host path.
 */
export function create(body) {
  return request('persistent-roots', { method: 'POST', body });
}

/**
 * Mount a root into a sandbox on the same node.
 *
 * A second read-write mount of the same root is a `409 mount.conflict` naming
 * the sandbox that holds it; read-only mounts are unlimited.
 */
export function mount(id, body) {
  return request(`persistent-roots/${id}/mount`, { method: 'POST', body });
}

/** Remove one sandbox's mount of a root. */
export function dismount(id, sandboxId) {
  return request(`persistent-roots/${id}/dismount`, {
    method: 'POST',
    body: { sandbox_id: sandboxId },
  });
}

/** The stored measurement, or an honest "not measured". */
export function usage(id) {
  return request(`persistent-roots/${id}/usage`);
}

/**
 * Archive the root (default) or delete its record.
 *
 * Archiving hides the root and keeps the data; `purge` deletes the row and
 * still leaves the directory on the node. Either way a mounted root is refused
 * with `409 root.in_use`.
 */
export function remove(id, purge = false) {
  return request(
    `persistent-roots/${id}${query({ purge: purge ? 'true' : undefined })}`,
    { method: 'DELETE' },
  );
}

/** The backups of one root, newest first. */
export function backups(rootId) {
  return request(`persistent-roots/${rootId}/backups`);
}

/** Back a root up now. `note` is an optional reminder of up to 200 characters. */
export function createBackup(rootId, note) {
  return request(`persistent-roots/${rootId}/backups`, { method: 'POST', body: { note: note || null } });
}

/**
 * Restore a backup. By default into the root it came from; `replace` moves the current
 * content aside first, `force` restores although a running sandbox writes to the root.
 */
export function restoreBackup(id, { rootId = null, replace = false, force = false } = {}) {
  return request(`backups/${id}/restore`, { method: 'POST', body: { root_id: rootId, replace, force } });
}

/** Delete a backup and its archive on the node. */
export function deleteBackup(id) {
  return request(`backups/${id}`, { method: 'DELETE' });
}
