// Media rendering: which model answers an image, video or music request.

import { request } from './client.js';

export const settings = () => request('media/settings');
export const saveSettings = (body) => request('media/settings', { method: 'PUT', body });
export const sandbox = (sandboxId) => request(`sandboxes/${sandboxId}/media`);
export const saveSandbox = (sandboxId, body) =>
  request(`sandboxes/${sandboxId}/media`, { method: 'PUT', body });
export const renders = (sandboxId, limit = 50) =>
  request(`sandboxes/${sandboxId}/media/renders?limit=${encodeURIComponent(limit)}`);

// Who may see and choose which media provider. Admin only.
export const users = () => request('media/users');
export const setUserGrant = (userId, body) =>
  request(`media/users/${userId}`, { method: 'PUT', body });

// Admin: remove evidence and the file it points at.
export const removeRender = (renderId) => request(`media/renders/${renderId}`, { method: 'DELETE' });
export const purge = (days) => request('media/purge', { method: 'POST', body: { days } });
