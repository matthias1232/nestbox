// Baselines, agent packs, and the tool catalog: the data a recipe is made of.
//
// Packs are data, not code (ADR 0020), so the console lists what the server
// knows and never invents a tool or a pack key. The catalog is asked for a
// baseline because the package name of a tool depends on the baseline's
// package manager.

import { request, query } from './client.js';

/** The base images a recipe can start from. */
export function baselines() {
  return request('baselines');
}

/** Agent packs, optionally only those a baseline supports. */
export function agentPacks(baseline) {
  return request(`agent-packs${query({ baseline })}`);
}

/** Tools, resolved against a baseline's package manager when one is named. */
export function toolCatalog(baseline) {
  return request(`tool-catalog${query({ baseline })}`);
}

/**
 * The live package catalogue of one baseline's distribution.
 *
 * The server asks the distribution's own package index, so this is a question,
 * not a stored catalog: `available: false` means the index could not be reached
 * and `detail` says why, rather than an empty list that looks like "no match".
 */
export function catalogue(baseline, q) {
  return request(`packages${query({ baseline, q })}`);
}
