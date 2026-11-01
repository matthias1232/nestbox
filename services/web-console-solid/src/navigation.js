// The console menu as data: which route belongs under which heading, and who sees it.
//
// A menu that lives inside the component is a menu nobody can test. Keeping it here
// means the grouping rules (every route in exactly one group, a hidden entry for a
// role that may not use it) are checked by a plain unit test, and a new route is one
// line in one place.
//
// The five groups answer five different questions an operator asks:
//   status   - what is happening right now?
//   operate  - what do I do with my sandboxes and data day to day?
//   setup    - what do I connect, build or configure once?
//   security - what may the sandboxes reach, and who decided that?
//   admin    - who may use this, and what does it cost?

/** Group headings in display order. */
export const GROUPS = [
  { id: 'status', label: 'Status' },
  { id: 'operate', label: 'Operations' },
  { id: 'setup', label: 'Setup' },
  { id: 'security', label: 'Security' },
  { id: 'admin', label: 'Administration' },
];

/**
 * Every route. `minRole` hides an entry from a role below it; the API still
 * enforces the same rule, so hiding is convenience and not protection.
 */
export const ROUTES = [
  { id: 'live', label: 'Agent situation', group: 'status' },
  { id: 'traffic', label: 'Live traffic', group: 'status' },
  { id: 'logs', label: 'Live logs', group: 'status' },
  { id: 'usage', label: 'Usage & cost', group: 'status' },
  { id: 'system', label: 'System containers', group: 'status', minRole: 'operator' },

  { id: 'sandboxes', label: 'Sandboxes', group: 'operate' },
  { id: 'persistent', label: 'Persistent data', group: 'operate' },
  { id: 'moves', label: 'Moves between nodes', group: 'operate' },
  { id: 'models', label: 'Models', group: 'operate' },
  { id: 'operations', label: 'Jobs & schedules', group: 'operate' },

  { id: 'tenants', label: 'Data centers & sites', group: 'setup' },
  { id: 'nodes', label: 'Nodes', group: 'setup' },
  { id: 'hosting', label: 'Rented machines', group: 'setup', minRole: 'operator' },
  { id: 'cluster', label: 'Cluster', group: 'setup' },
  { id: 'images', label: 'Image registry & sync', group: 'setup' },
  { id: 'providers', label: 'Providers & keys', group: 'setup', minRole: 'operator' },
  { id: 'mcp', label: 'MCP servers', group: 'setup' },
  { id: 'media', label: 'Media rendering', group: 'setup' },
  { id: 'skills', label: 'Skills', group: 'setup' },
  { id: 'cudax', label: 'CUDA-X', group: 'setup' },
  { id: 'builder', label: 'Build image', group: 'setup' },
  { id: 'recipes', label: 'Recipes', group: 'setup' },

  { id: 'policies', label: 'Policies', group: 'security' },
  { id: 'privacy', label: 'Content privacy', group: 'security' },
  { id: 'master', label: 'Master control', group: 'security' },
  { id: 'proxies', label: 'Exit proxies', group: 'security' },

  // Only an admin manages who may sign in; the page and the API both refuse anyone else.
  { id: 'users', label: 'Users & roles', group: 'admin', minRole: 'admin' },
];

const RANK = { viewer: 1, operator: 2, admin: 3 };

/** True when `role` may see `route`. An unknown role sees only unrestricted entries. */
export function visible(route, role) {
  if (!route.minRole) return true;
  return (RANK[role] ?? 0) >= RANK[route.minRole];
}

/** The menu for a role: groups in order, each with its visible routes, empty groups dropped. */
export function menuFor(role) {
  return GROUPS.map((group) => ({
    ...group,
    routes: ROUTES.filter((route) => route.group === group.id && visible(route, role)),
  })).filter((group) => group.routes.length > 0);
}
