// Model catalog and hardware.

import { request, query } from './client.js';

export function catalog(refresh = false) {
  return request(`models${query({ refresh_hardware: refresh ? 'true' : undefined })}`);
}

export function installed() {
  return request('models/installed');
}

export function hardware(refresh = false) {
  return request(`models/hardware${query({ refresh_hardware: refresh ? 'true' : undefined })}`);
}

export function install(key) {
  return request(`models/${key}/install`, { method: 'POST' });
}

export function uninstall(key) {
  return request(`models/${key}/uninstall`, { method: 'POST' });
}
