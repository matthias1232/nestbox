// Gateways, supervisors, and topology.

import { request } from './client.js';

export function gateways() {
  return request('gateways');
}

export function gateway(id) {
  return request(`gateways/${id}`);
}

export function updateSettings(id, settings, version) {
  return request(`gateways/${id}/settings`, {
    method: 'PUT',
    body: { settings, version },
  });
}

export function sync(id) {
  return request(`gateways/${id}/sync`, { method: 'POST' });
}

export function supervisors() {
  return request('supervisors');
}

export function topology() {
  return request('topology');
}

// The settings document of the supervisor that watches one sandbox.
export function supervisorSettings(sandboxId) {
  return request(`sandboxes/${sandboxId}/settings`);
}

// Replace that document. The sandbox receives it when it is running.
export function setSupervisorSettings(sandboxId, settings) {
  return request(`sandboxes/${sandboxId}/settings`, {
    method: 'PUT',
    body: settings,
  });
}

// Queue one whitelisted action for the sandbox's supervisor.
export function supervisorAction(sandboxId, kind, payload = {}) {
  return request(`sandboxes/${sandboxId}/supervisor/actions`, {
    method: 'POST',
    body: { kind, payload },
  });
}
