// Content privacy: the DLP policy, a single test of the filter, the incidents it
// records, and the admin purge.

import { request, query } from './client.js';

// The environment default (no sandbox id) or one sandbox's own settings.
// The answer carries both the stored settings and the detector catalog.
export const settings = (sandboxId) =>
  request(`privacy/settings${query({ sandbox_id: sandboxId })}`);

// Replaces the whole settings row, so the body always carries every field.
export const saveSettings = (body) => request('privacy/settings', { method: 'PUT', body });

// One test of the filter with the policy in force. It answers what would
// happen; the console never sends the content anywhere else.
export const inspect = (body) => request('privacy/inspect', { method: 'POST', body });

export const incidents = (sandboxId, limit = 50) =>
  request(`privacy/incidents${query({ sandbox_id: sandboxId, limit })}`);

// Admin: remove incident rows older than a day count.
export const purge = (days) => request('privacy/purge', { method: 'POST', body: { days } });
