// Node state: the list, and one node with its sandboxes.
//
// No JSX here, so the arithmetic-free parts stay testable without a DOM. Every
// number this file returns came from the server: the page draws budgets, it
// never reconstructs one from a hardware profile.

import { createEffect, createSignal } from 'solid-js';
import { gib } from '../../components/ResourceBars.jsx';
import * as api from '../../api/nodes.js';

/** The node list, with an explicit "loaded" flag so empty and loading differ. */
export function useNodes() {
  const [items, setItems] = createSignal([]);
  const [loading, setLoading] = createSignal(false);
  const [loaded, setLoaded] = createSignal(false);
  const [error, setError] = createSignal(null);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const page = await api.list();
      setItems(page?.items || []);
      setLoaded(true);
    } catch (failure) {
      setError(failure.message);
    } finally {
      setLoading(false);
    }
  }

  return { items, loading, loaded, error, load };
}

/**
 * One node, its sandboxes, and the two writes the drawer offers.
 *
 * `options.onChanged` is called after a successful probe or rename so the list
 * behind the drawer cannot keep showing the values the drawer just replaced.
 */
export function useNodeDetail(nodeId, options = {}) {
  const [node, setNode] = createSignal(null);
  const [sandboxes, setSandboxes] = createSignal([]);
  const [total, setTotal] = createSignal(0);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [probing, setProbing] = createSignal(false);
  const [saving, setSaving] = createSignal(false);

  createEffect(() => {
    const id = nodeId();
    setNode(null);
    setSandboxes([]);
    setTotal(0);
    setError(null);
    setNotice(null);
    if (id) load(id);
  });

  async function load(id) {
    setLoading(true);
    setError(null);
    try {
      // Both answers are needed before the drawer is useful, so they are asked
      // for together rather than one after the other.
      const [view, page] = await Promise.all([api.get(id), api.sandboxes(id)]);
      setNode(view);
      setSandboxes(page?.items || []);
      setTotal(page?.total_estimate ?? (page?.items || []).length);
    } catch (failure) {
      setError({ code: failure.code, message: failure.message });
    } finally {
      setLoading(false);
    }
  }

  /**
   * Measure the node through its own runner.
   *
   * `node.runtime_unavailable` is kept as a code, not flattened into a string:
   * "the runner on this machine did not answer" is a state of the node the
   * operator can act on, and it must not look like a console bug.
   */
  async function probe() {
    const id = nodeId();
    if (!id || probing()) return;
    setProbing(true);
    setError(null);
    setNotice(null);
    try {
      const measured = await api.probe(id);
      setNotice(describeMeasurement(measured));
      await load(id);
      options.onChanged?.();
    } catch (failure) {
      setError({ code: failure.code, message: failure.message });
    } finally {
      setProbing(false);
    }
  }

  /** Rename the node and set its description. Only those two fields exist. */
  async function saveDisplay(displayName, description) {
    const id = nodeId();
    if (!id || saving()) return false;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await api.updateDisplay(id, displayName, description);
      setNode(updated);
      setNotice(`Saved. This node is now "${updated.display_name}".`);
      options.onChanged?.();
      return true;
    } catch (failure) {
      setError({ code: failure.code, message: failure.message });
      return false;
    } finally {
      setSaving(false);
    }
  }

  return {
    node, sandboxes, total, loading, error, notice, probing, saving,
    load: () => load(nodeId()),
    probe, saveDisplay,
    setError, setNotice,
  };
}

/** A sentence about what the runner answered, built only from its fields. */
function describeMeasurement(measured) {
  const parts = [];
  if (measured?.cpu_cores) parts.push(`${measured.cpu_cores} cores`);
  if (measured?.system_ram_bytes) parts.push(`${gib(measured.system_ram_bytes)} RAM`);
  parts.push(measured?.has_gpu ? `${measured.gpu_count || 0} GPU(s)` : 'no GPU');
  const host = measured?.hostname ? ` ${measured.hostname}` : ' this node';
  return `Measured${host}: ${parts.join(' · ')}.`;
}
