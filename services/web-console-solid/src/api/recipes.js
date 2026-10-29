// Recipes: the desired state of a sandbox image, and the build behind it.
//
// A recipe is a template. Building it produces an image tagged by the content
// hash of the recipe, so the tag is immutable: an edit changes the hash and
// therefore the reference, and the image behind an existing tag is never
// rewritten. Nothing here decides that; it is the server's rule and this file
// only names the calls.

import { request } from './client.js';

/** Every recipe in the environment. The list carries the whole recipe. */
export function list() {
  return request('recipes');
}

/**
 * Create a draft recipe.
 *
 * The body names catalog keys, never free text: a tool or pack the catalog does
 * not know is refused with a 409 instead of becoming a row that cannot build.
 */
export function create(body) {
  return request('recipes', { method: 'POST', body });
}

/** One recipe, fresh from the store. */
export function get(id) {
  return request(`recipes/${id}`);
}

/**
 * Replace the editable fields of a recipe.
 *
 * The whole recipe is sent rather than a patch of fields, because "keep what I
 * did not send" is how a field silently disappears. `node` is omitted on
 * purpose: the server keeps the existing node, so an edit cannot move a recipe
 * to another machine.
 */
export function update(id, body) {
  return request(`recipes/${id}`, { method: 'PATCH', body });
}

/** Delete the recipe. Sandboxes created from it keep running. */
export function remove(id) {
  return request(`recipes/${id}`, { method: 'DELETE' });
}

/**
 * Build the image.
 *
 * The answer is either `{ job_id, reused: false, recipe }` for a queued build
 * or `{ job_id: null, reused: true, recipe }` when the same content already has
 * an image; the caller must say which, because "nothing was built" is not a
 * failure.
 */
export function build(id) {
  return request(`recipes/${id}/build`, { method: 'POST' });
}

/** Create a sandbox from a built recipe. Returns the sandbox. */
export function instantiate(id, name) {
  return request(`recipes/${id}/instantiate`, { method: 'POST', body: { name } });
}

/**
 * Copy a template's selection onto a recipe.
 *
 * The recipe keeps its agents, resources and mounts; the server clears the built
 * image because the previous one does not contain the new selection.
 */
export function applyTemplate(id, templateId) {
  return request(`recipes/${id}/apply-template`, { method: 'POST', body: { template_id: templateId } });
}
