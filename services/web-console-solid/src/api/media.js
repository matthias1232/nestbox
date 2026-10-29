// Media rendering: which model answers an image, video or music request.

import { request } from './client.js';

export const settings = () => request('media/settings');
export const saveSettings = (body) => request('media/settings', { method: 'PUT', body });
export const sandbox = (sandboxId) => request(`sandboxes/${sandboxId}/media`);
export const saveSandbox = (sandboxId, body) =>
  request(`sandboxes/${sandboxId}/media`, { method: 'PUT', body });
export const renders = (sandboxId, limit = 50) =>
  request(`sandboxes/${sandboxId}/media/renders?limit=${encodeURIComponent(limit)}`);
