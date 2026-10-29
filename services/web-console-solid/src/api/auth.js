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

// A new CSRF token for the current session. The console keeps it in memory only, so after a
// page reload this is how it gets one without asking for the password again.
export function csrf() {
  return request('auth/csrf', { method: 'POST' });
}
