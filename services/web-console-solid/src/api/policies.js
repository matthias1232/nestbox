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

// The sections and fields a policy document may contain; the forms render from this.
export function schema() {
  return request('policy-schema');
}

export function templates() {
  return request('policy-templates');
}

export function instantiateTemplate(id, assignTo = null) {
  return request(`policy-templates/${id}/instantiate`, { method: 'POST', body: { assign_to: assignTo } });
}

export function dnsRules() {
  return request('dns-rules');
}

export function createDnsRule(body) {
  return request('dns-rules', { method: 'POST', body });
}

export function removeDnsRule(id) {
  return request(`dns-rules/${id}`, { method: 'DELETE' });
}

export function sandboxPolicy(sandboxId) {
  return request(`sandboxes/${sandboxId}/policy`);
}

export function assignPolicy(sandboxId, policyId) {
  return request(`sandboxes/${sandboxId}/policy`, { method: 'PUT', body: { policy_id: policyId } });
}

export function unassignPolicy(sandboxId) {
  return request(`sandboxes/${sandboxId}/policy`, { method: 'DELETE' });
}

export function sandboxRules(sandboxId) {
  return request(`sandboxes/${sandboxId}/rules`);
}

export function egressMode(sandboxId) {
  return request(`sandboxes/${sandboxId}/egress-mode`);
}

export function setEgressMode(sandboxId, mode) {
  return request(`sandboxes/${sandboxId}/egress-mode`, { method: 'PUT', body: { mode } });
}
