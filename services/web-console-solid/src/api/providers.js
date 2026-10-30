// Provider endpoint profiles.
//
// The console never sends or receives a credential value, only a locator. That
// is why there is no `setKey` function here: a key is placed on the host and the
// profile points at it.

import { request } from './client.js';

export function list() {
  return request('providers');
}

export function get(id) {
  return request(`providers/${id}`);
}

export function create(body) {
  return request('providers', { method: 'POST', body });
}

export function update(id, body) {
  return request(`providers/${id}`, { method: 'PUT', body });
}

export function remove(id) {
  return request(`providers/${id}`, { method: 'DELETE' });
}

/** Probe every endpoint and persist each verdict; answers in failover order. */
export function probeAll() {
  return request('providers/probe-all', { method: 'POST' });
}

export function probe(id) {
  return request(`providers/${id}/probe`, { method: 'POST' });
}
