// Sandbox resource.

import { request, query } from './client.js';

export function list(params = {}) {
  return request(`sandboxes${query(params)}`);
}

export function get(id) {
  return request(`sandboxes/${id}`);
}

export function create(body) {
  return request('sandboxes', { method: 'POST', body });
}

export function act(id, action) {
  return request(`sandboxes/${id}/actions`, { method: 'POST', body: { action } });
}

export function remove(id) {
  return request(`sandboxes/${id}`, { method: 'DELETE' });
}

export function logs(id, tail = 200) {
  return request(`sandboxes/${id}/logs${query({ tail })}`);
}

export function inference(id, limit = 50) {
  return request(`sandboxes/${id}/inference${query({ limit })}`);
}

export function rules(id) {
  return request(`sandboxes/${id}/rules`);
}
