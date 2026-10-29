// CUDA-X components and their installation on one node.
//
// Assigning and installing are separate: an assignment states what a node
// should have, an install fetches the declared images. Neither is blocked by a
// hardware warning; the answer carries the warnings instead.

import { request, query } from './client.js';

/** The editable component catalog. */
export function components() {
  return request('cuda-x/components');
}

/** Installations, optionally for one node. */
export function installs(node) {
  return request(`cuda-x/installs${query({ node })}`);
}

/**
 * State that this node should have the component.
 *
 * The answer is the installation row with its `assessment`; warnings do not
 * refuse the request.
 */
export function assign(key, node, force = false) {
  return request(`cuda-x/components/${key}/assign`, {
    method: 'POST',
    body: { node, force },
  });
}

/** Pull the component's declared images on the node. */
export function install(key, node, force = false) {
  return request(`cuda-x/components/${key}/install`, {
    method: 'POST',
    body: { node, force },
  });
}

/** Remove the assignment. Software already on the node stays. */
export function remove(key, node) {
  return request(`cuda-x/components/${key}/remove`, {
    method: 'POST',
    body: { node },
  });
}
