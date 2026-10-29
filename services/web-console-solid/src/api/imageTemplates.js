// Image templates: saved builder selections a team can reuse.
//
// A template carries choices (tools, packages, repositories, MCP servers), never
// an image. The server decides who may see an unshared template; this file only
// names the calls.

import { request } from './client.js';

/** Templates this actor may use: shared ones, its own, and — as admin — all. */
export const list = () => request('image-templates');

/** The environment default, or `{ template: null }` when none is marked. */
export const defaultTemplate = () => request('image-templates/default');

/** Save the current selection as a template. */
export const create = (body) => request('image-templates', { method: 'POST', body });

/** Replace a template's selection. */
export const update = (id, body) => request(`image-templates/${id}`, { method: 'PATCH', body });

/** Delete a template. Recipes keep the copy they applied. */
export const remove = (id) => request(`image-templates/${id}`, { method: 'DELETE' });
