// Persistent root state.
//
// The server derives the host path and owns the one-read-write rule, so this
// hook sends a kind, a node, and a slug, and then repeats what the server
// answered. A 409 names the sandbox that holds a mount; the message is shown as
// it came rather than translated into a guess.

import { createMemo, createSignal } from 'solid-js';
import * as api from '../../api/persistent.js';
import * as nodesApi from '../../api/nodes.js';
import * as sandboxesApi from '../../api/sandboxes.js';

const KINDS = {
  agent_home: 'agent home',
  project: 'project',
};

export function usePersistent() {
  const [items, setItems] = createSignal([]);
  const [nodes, setNodes] = createSignal([]);
  const [nodesKnown, setNodesKnown] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [errorCode, setErrorCode] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [selectedId, setSelectedId] = createSignal(null);
  const [usage, setUsage] = createSignal(null);
  const [sandboxes, setSandboxes] = createSignal([]);
  const [sandboxesError, setSandboxesError] = createSignal(null);
  const [form, setForm] = createSignal({ kind: 'project', node: '', slug: '' });
  const [sandboxId, setSandboxId] = createSignal('');
  const [mountPath, setMountPath] = createSignal('');
  const [readOnly, setReadOnly] = createSignal(false);

  const selected = createMemo(() => items().find((root) => root.id === selectedId()) || null);
  const nodeIndex = createMemo(() => new Map(nodes().map((node) => [node.id, node])));

  function fail(failure) {
    setError(failure.message);
    setErrorCode(failure.code || null);
  }

  function clearMessages() {
    setError(null);
    setErrorCode(null);
    setNotice(null);
  }

  /** A node's name, or a short id when the name cannot be read. */
  function nodeLabel(id) {
    if (!id) return '—';
    const node = nodeIndex().get(id);
    if (node) return node.display_name || node.name;
    return `${String(id).slice(0, 8)}${nodesKnown() ? ' (unknown node)' : '…'}`;
  }

  function kindLabel(kind) {
    return KINDS[kind] || kind;
  }

  async function load() {
    setLoading(true);
    clearMessages();
    try {
      setItems((await api.list()).items || []);
    } catch (failure) {
      fail(failure);
    } finally {
      setLoading(false);
    }
    await loadNodes();
  }

  async function refresh() {
    try {
      setItems((await api.list()).items || []);
    } catch (failure) {
      fail(failure);
    }
  }

  async function loadNodes() {
    try {
      setNodes((await nodesApi.list()).items || []);
    } catch {
      // Node names are a convenience; without node read the rows show a short id.
      setNodes([]);
    } finally {
      setNodesKnown(true);
    }
  }

  async function open(root) {
    clearMessages();
    setSelectedId(root.id);
    setUsage(null);
    setSandboxId('');
    setMountPath('');
    setReadOnly(false);
    await Promise.all([loadUsage(root.id), loadSandboxes(root)]);
  }

  function close() {
    clearMessages();
    setSelectedId(null);
    setUsage(null);
    setSandboxes([]);
    setSandboxesError(null);
  }

  async function loadUsage(id) {
    try {
      setUsage(await api.usage(id));
    } catch {
      // The measurement is a nice-to-have next to the row's own number; a failed
      // read must not hide the mount form.
      setUsage(null);
    }
  }

  async function loadSandboxes(root) {
    setSandboxesError(null);
    try {
      const page = await sandboxesApi.list({ node: root.node_id, limit: 100 });
      setSandboxes(page.items || []);
    } catch (failure) {
      setSandboxes([]);
      setSandboxesError(failure.message);
    }
  }

  function updateForm(patch) {
    setForm({ ...form(), ...patch });
  }

  async function create(event) {
    if (event) event.preventDefault();
    const value = form();
    const slug = value.slug.trim();
    if (!slug) {
      setError('A root needs a slug: lowercase letters, digits, and hyphens.');
      setErrorCode('root.slug_required');
      return;
    }
    setBusy(true);
    clearMessages();
    try {
      const root = await api.create({
        kind: value.kind,
        node: value.node || undefined,
        slug,
      });
      setNotice(
        `Root “${root.slug}” created at ${root.host_path}. The path is derived by the server `
        + `from kind, owner, and slug; it was not a field in this form.`,
      );
      setForm({ ...value, slug: '' });
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function mount(root) {
    const path = mountPath().trim();
    if (!sandboxId()) {
      setError('Choose the sandbox to mount the root into.');
      setErrorCode('mount.sandbox_required');
      return;
    }
    if (!path) {
      setError('A mount path is required: the absolute path inside the sandbox.');
      setErrorCode('mount.path_required');
      return;
    }
    setBusy(true);
    clearMessages();
    try {
      const created = await api.mount(root.id, {
        sandbox_id: sandboxId(),
        mount_path: path,
        read_only: readOnly(),
      });
      setNotice(
        `Mounted at ${created.mount_path} (${created.read_only ? 'read-only' : 'read-write'}). `
        + 'An existing container is not changed: the bind applies the next time its container is created.',
      );
      setMountPath('');
      setReadOnly(false);
      await refresh();
      await loadUsage(root.id);
    } catch (failure) {
      // The interesting failure is 409 mount.conflict: the message names the
      // sandbox that already holds the root read-write. Keep the form so the
      // operator can retry read-only without typing the path again.
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function dismount(root, mounted) {
    setBusy(true);
    clearMessages();
    try {
      await api.dismount(root.id, mounted.sandbox_id);
      setNotice(
        `Dismounted from “${mounted.sandbox_name}”. The files stay in ${root.host_path}; `
        + 'dismounting removes the bind, not the data.',
      );
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function archive(root) {
    setBusy(true);
    clearMessages();
    try {
      const removal = await api.remove(root.id, false);
      close();
      setNotice(removal.note);
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function purge(root) {
    setBusy(true);
    clearMessages();
    try {
      const removal = await api.remove(root.id, true);
      close();
      setNotice(removal.note);
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  return {
    items, nodes, loading, busy, error, errorCode, notice, selected, usage,
    sandboxes, sandboxesError, form, updateForm, create,
    sandboxId, setSandboxId, mountPath, setMountPath, readOnly, setReadOnly,
    load, open, close, mount, dismount, archive, purge, nodeLabel, kindLabel,
  };
}

/** Bytes as measured, or "not measured" — never a zero for an unmeasured root. */
export function usageLabel(root) {
  if (!root || root.measured_at == null || root.bytes_used == null) return 'not measured';
  const bytes = Number(root.bytes_used);
  if (!Number.isFinite(bytes)) return 'not measured';
  if (bytes === 0) return '0 B';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} B` : `${value.toFixed(1)} ${units[unit]}`;
}
