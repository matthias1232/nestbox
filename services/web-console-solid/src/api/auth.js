// Authentication and environment membership.

import { request } from './client.js';

export function login(email, password) {
  return request('auth/login', { method: 'POST', body: { email, password } });
}

export function logout() {
  return request('auth/logout', { method: 'POST' });
}

export function me() {
  return request('auth/me');
}

export function environments() {
  return request('environments');
}

export function currentEnvironment() {
  return request('environments/current');
}
