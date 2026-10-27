// Nodes: the machines sandboxes and models run on.
//
// The list answer is an array, not a page: the number of nodes is small by
// design, and the server already computed each budget, so the console never sums
// installs itself.

import { request } from './client.js';

/** Every node with its budget and sandbox count. */
export function list() {
  return request('nodes');
}
