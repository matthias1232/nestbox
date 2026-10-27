// Model catalog, hardware, and the facets behind the filter chips.

import { request, query } from './client.js';

/**
 * The catalog, classified against this machine.
 *
 * `refresh` forces a fresh probe. The filters are passed to the server so a
 * caller can ask for "only what fits" without loading the rest; the console
 * applies its live filters to the verdicts the server returned.
 */
export function catalog(refresh = false, filters = {}, node) {
  return request(`models${query({
    refresh_hardware: refresh ? 'true' : undefined,
    node,
    family: filters.family,
    publisher: filters.publisher,
    runtime: filters.runtime,
    cpu_only: filters.cpuOnly ? 'true' : undefined,
    unified_only: filters.unifiedOnly ? 'true' : undefined,
    fits: filters.fits === undefined ? undefined : String(filters.fits),
  })}`);
}

/**
 * Every install in the environment, one row per model per node.
 *
 * The row carries `model_key`, `model_name`, `node_id`, and `node_name`, so a
 * screen that only holds an install id can name the model and the machine
 * without a second call.
 */
export function installs(node) {
  return request(`models/installs${query({ node })}`);
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

/**
 * Install on a node. The answer is the reservation it made, or
 * `422 node.insufficient_resources` naming what the node's other installs hold.
 */
export function install(key, node) {
  return request(`models/${key}/install`, { method: 'POST', body: node ? { node } : {} });
}

export function uninstall(key, node) {
  return request(`models/${key}/uninstall`, { method: 'POST', body: node ? { node } : {} });
}

/**
 * Refresh the catalogue from Hugging Face, Ollama, and NGC.
 *
 * Runs inside the request, so the counts come back with it. A curated row is
 * never overwritten; the per-source counts say what changed.
 */
export function sync() {
  return request('models/sync', { method: 'POST' });
}
