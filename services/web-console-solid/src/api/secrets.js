// The secret store: write-only API keys.
//
// A value is sent once, in `put`. Nothing here returns one: the list carries the last
// four characters, the locator a provider points at (`db://<name>`), and how many
// providers use the key.

import { request } from './client.js';

export function list() {
  return request('secrets');
}

export function put({ name, kind, value }) {
  return request('secrets', { method: 'POST', body: { name, kind, value } });
}

export function remove(id) {
  return request(`secrets/${id}`, { method: 'DELETE' });
}
