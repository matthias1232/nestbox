// Jobs and schedules.

import { request, query } from './client.js';

export function list(limit = 50) {
  return request(`jobs${query({ limit })}`);
}

export function get(id) {
  return request(`jobs/${id}`);
}

export function logs(id, limit = 200) {
  return request(`jobs/${id}/logs${query({ limit })}`);
}

export function schedules() {
  return request('schedules');
}

export function runSchedule(id) {
  return request(`schedules/${id}/run`, { method: 'POST' });
}
