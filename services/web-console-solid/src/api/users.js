// User administration (admin only): who may sign in here, with which role.

import { request } from './client.js';

export const list = () => request('users');
export const create = (body) => request('users', { method: 'POST', body });
export const update = (id, body) => request(`users/${id}`, { method: 'PUT', body });
export const setPassword = (id, password) =>
  request(`users/${id}/password`, { method: 'POST', body: { password } });
export const remove = (id) => request(`users/${id}`, { method: 'DELETE' });
