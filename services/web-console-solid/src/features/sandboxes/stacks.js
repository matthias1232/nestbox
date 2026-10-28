// Grouping sandboxes into stacks for the list.
//
// A stack is a lead and its members (`parent_id` points at the lead), one level
// deep. The list is paged by the server, so a member can arrive without its lead
// on the page; that member is shown as a standalone row that says whose it is,
// rather than being hidden or shown under a lead that is not there.

/** The worst of a set of statuses, so a collapsed stack still reports trouble. */
const SEVERITY = ['failed', 'degraded', 'starting', 'stopping', 'provisioning', 'pending', 'stopped', 'running'];

export function worstStatus(statuses) {
  for (const status of SEVERITY) {
    if (statuses.includes(status)) return status;
  }
  return statuses[0] || 'unknown';
}

/**
 * Turn the flat page into display rows, in the order the server sent the leads.
 *
 * Each row is `{ sandbox, role, members, orphanOf }`:
 * - `role: 'lead'` carries its loaded `members`;
 * - `role: 'single'` stands alone, with `orphanOf` set to the lead's id when it
 *   is a member whose lead is on another page;
 * - members of a lead are listed inside it and do not appear as rows of their own.
 */
export function groupStacks(items) {
  const byId = new Map(items.map((sandbox) => [sandbox.id, sandbox]));
  const members = new Map();
  for (const sandbox of items) {
    if (sandbox.parent_id && byId.has(sandbox.parent_id)) {
      if (!members.has(sandbox.parent_id)) members.set(sandbox.parent_id, []);
      members.get(sandbox.parent_id).push(sandbox);
    }
  }

  const rows = [];
  for (const sandbox of items) {
    if (sandbox.parent_id && byId.has(sandbox.parent_id)) continue;
    const own = members.get(sandbox.id) || [];
    own.sort((a, b) => a.name.localeCompare(b.name));
    if (own.length > 0) {
      rows.push({ sandbox, role: 'lead', members: own, orphanOf: null });
    } else {
      rows.push({ sandbox, role: 'single', members: [], orphanOf: sandbox.parent_id || null });
    }
  }
  return rows;
}

/** The sandboxes a given sandbox may join: other standalone sandboxes or leads. */
export function possibleLeads(items, sandbox) {
  const hasMembers = items.some((other) => other.parent_id === sandbox.id);
  if (hasMembers) return [];
  return items.filter((other) => other.id !== sandbox.id && !other.parent_id);
}
