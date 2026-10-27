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
