// Model catalog, hardware, and the facets behind the filter chips.

import { request, query } from './client.js';

/**
 * The catalog, classified against this machine.
 *
 * `refresh` forces a fresh probe. The filters are passed to the server so a
 * caller can ask for "only what fits" without loading the rest; the console
 * applies its live filters to the verdicts the server returned.
 */
export function catalog(refresh = false, filters = {}) {
  return request(`models${query({
    refresh_hardware: refresh ? 'true' : undefined,
    family: filters.family,
    publisher: filters.publisher,
    runtime: filters.runtime,
    cpu_only: filters.cpuOnly ? 'true' : undefined,
    unified_only: filters.unifiedOnly ? 'true' : undefined,
    fits: filters.fits === undefined ? undefined : String(filters.fits),
  })}`);
}

export function installed() {
  return request('models/installed');
}

export function hardware(refresh = false) {
  return request(`models/hardware${query({ refresh_hardware: refresh ? 'true' : undefined })}`);
}

/** One model as a requirement profile, for a detail view. */
export function requirements(key) {
  return request(`models/requirements${query({ key })}`);
}

/** Counts per family, publisher, runtime, and verdict. */
export function facets() {
  return request('models/facets');
}

export function install(key) {
  return request(`models/${key}/install`, { method: 'POST' });
}

export function uninstall(key) {
  return request(`models/${key}/uninstall`, { method: 'POST' });
}
