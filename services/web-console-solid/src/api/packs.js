// The agent-pack catalog.
//
// A pack decides the image and the command a sandbox or an agent is started
// from. The list is read-only and global (ADR 0013); asking the server rather
// than holding a table here is what keeps a new pack a row, not a release.

import { request, query } from './client.js';

/** The enabled agent packs, narrowed to those `baseline` supports. */
export function agentPacks(baseline) {
  return request(`agent-packs${query({ baseline })}`);
}
