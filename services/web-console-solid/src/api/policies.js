// Policies, network rules, drafts, and temporary windows.

import { request, query } from './client.js';

export function list() {
  return request('policies');
}

export function get(id) {
  return request(`policies/${id}`);
}

export function create(body) {
  return request('policies', { method: 'POST', body });
}

export function update(id, body) {
  return request(`policies/${id}`, { method: 'PUT', body });
}

export function remove(id) {
  return request(`policies/${id}`, { method: 'DELETE' });
}

export function diff(id, against) {
  return request(`policies/${id}/diff${query({ against })}`);
}

export function networkRules() {
  return request('network-rules');
}

export function createRule(body) {
  return request('network-rules', { method: 'POST', body });
}

export function removeRule(id) {
  return request(`network-rules/${id}`, { method: 'DELETE' });
}

export function drafts() {
  return request('rule-drafts');
}

export function draftFromEvent(event, sandboxId) {
  return request('rule-drafts', {
    method: 'POST',
    body: { source_kind: 'network', event, sandbox_id: sandboxId },
  });
}

export function decideDraft(id, approve, reason) {
  const action = approve ? 'approve' : 'reject';
  return request(`rule-drafts/${id}/${action}`, { method: 'POST', body: { reason } });
}

export function openWindow(sandboxId, minutes, reason) {
  return request('traffic-windows', {
    method: 'POST',
    body: { sandbox_id: sandboxId, minutes, reason },
  });
}

export function closeWindow(id) {
  return request(`traffic-windows/${id}`, { method: 'DELETE' });
}
