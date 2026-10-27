// Formatting shared by the node list and the node drawer.
//
// Two mappings, both about words rather than numbers: what colour a node status
// has, and how a node kind reads. They live here so the list and the drawer
// cannot disagree about what "bare_metal" is called.

const STATUS_TONE = {
  online: 'ok',
  degraded: 'warn',
  offline: 'bad',
  unknown: 'muted',
};

/** The badge colour for a node status; anything unclassified stays neutral. */
export function statusTone(status) {
  return STATUS_TONE[String(status || '').toLowerCase()] || 'muted';
}

const KIND_LABEL = {
  local: 'local host',
  vm: 'VM',
  wsl: 'WSL',
  bare_metal: 'bare metal',
  kubernetes: 'kubernetes',
};

/** A node kind in words; an unknown kind is shown as the server spelled it. */
export function kindLabel(kind) {
  return KIND_LABEL[kind] || String(kind || 'unknown').replaceAll('_', ' ');
}
