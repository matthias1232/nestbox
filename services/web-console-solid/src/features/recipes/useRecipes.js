// Recipe list state.
//
// The hook owns the calls, the polling after a build, and the sentence that
// tells the operator what happened. The page and the drawer only render, which
// is what lets the build outcome and the mount rules be read without a DOM.

import { createMemo, createSignal, onCleanup } from 'solid-js';
import * as api from '../../api/recipes.js';
import * as nodesApi from '../../api/nodes.js';
import * as packsApi from '../../api/packs.js';
import * as persistentApi from '../../api/persistent.js';

// A build runs detached on the server, so the row is the only place its outcome
// appears. Polling stops on the first non-building state and after two minutes:
// a screen that asks forever hides a job that will never finish.
const POLL_MS = 4000;
const POLL_LIMIT = 30;

export function useRecipes() {
  const [items, setItems] = createSignal([]);
  const [nodes, setNodes] = createSignal([]);
  const [nodesKnown, setNodesKnown] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [errorCode, setErrorCode] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [selectedId, setSelectedId] = createSignal(null);
  const [editing, setEditing] = createSignal(false);
  const [sandboxName, setSandboxName] = createSignal('');
  const [catalogs, setCatalogs] = createSignal({ baselines: [], packs: [], tools: [], roots: [] });
  const [catalogError, setCatalogError] = createSignal(null);

  // The list carries the whole recipe, so the drawer follows every reload
  // without a second copy of the same row.
  const selected = createMemo(() => items().find((recipe) => recipe.id === selectedId()) || null);
  const nodeIndex = createMemo(() => new Map(nodes().map((node) => [node.id, node])));

  let timer = null;
  let polls = 0;

  function stopPolling() {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }
  onCleanup(stopPolling);

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

  async function load() {
    setLoading(true);
    clearMessages();
    try {
      const result = await api.list();
      setItems(result.items || []);
    } catch (failure) {
      fail(failure);
    } finally {
      setLoading(false);
    }
    await loadNodes();
  }

  /** Reload the rows without the loading placeholder, for after an action. */
  async function refresh() {
    try {
      const result = await api.list();
      setItems(result.items || []);
    } catch (failure) {
      fail(failure);
    }
  }

  async function loadNodes() {
    try {
      setNodes(await nodesApi.list());
    } catch {
      // Node names are a convenience. A role without node read still sees every
      // recipe, with a short node id instead of a name.
      setNodes([]);
    } finally {
      setNodesKnown(true);
    }
  }

  function merge(recipe) {
    setItems((current) => current.map((row) => (row.id === recipe.id ? recipe : row)));
  }

  async function open(recipe) {
    stopPolling();
    clearMessages();
    setEditing(false);
    setCatalogError(null);
    setSandboxName(defaultSandboxName(recipe.name));
    setSelectedId(recipe.id);
    try {
      // The row is already the whole recipe; the get is for a build error and a
      // state that changed while the list sat on the screen.
      merge(await api.get(recipe.id));
    } catch (failure) {
      fail(failure);
    }
  }

  function close() {
    stopPolling();
    setSelectedId(null);
    setEditing(false);
    setCatalogError(null);
    clearMessages();
  }

  /**
   * Open the editor and the catalogs it needs.
   *
   * The catalog is loaded here rather than in the editor so the editor stays a
   * form over values it was given, and so a failed catalog read is one sentence
   * in one place instead of a half-populated picker.
   */
  async function startEdit(recipe) {
    clearMessages();
    setCatalogError(null);
    setEditing(true);
    const [baselines, packs, tools, roots] = await Promise.allSettled([
      packsApi.baselines(),
      packsApi.agentPacks(recipe.baseline_key),
      packsApi.toolCatalog(recipe.baseline_key),
      persistentApi.list(),
    ]);
    setCatalogs({
      baselines: baselines.status === 'fulfilled' ? baselines.value.items || [] : [],
      packs: packs.status === 'fulfilled' ? packs.value.items || [] : [],
      tools: tools.status === 'fulfilled' ? tools.value.items || [] : [],
      roots: roots.status === 'fulfilled' ? roots.value : [],
    });
    const failures = [baselines, packs, tools, roots].filter((one) => one.status === 'rejected');
    if (failures.length > 0) {
      setCatalogError(failures.map((one) => one.reason?.message || 'read failed').join(' '));
    }
  }

  /** Reload the pack and tool lists for a new baseline, keeping the draft. */
  async function reloadCatalog(baseline) {
    setCatalogError(null);
    const [packs, tools] = await Promise.allSettled([
      packsApi.agentPacks(baseline),
      packsApi.toolCatalog(baseline),
    ]);
    setCatalogs((current) => ({
      ...current,
      packs: packs.status === 'fulfilled' ? packs.value.items || [] : current.packs,
      tools: tools.status === 'fulfilled' ? tools.value.items || [] : current.tools,
    }));
    const failures = [packs, tools].filter((one) => one.status === 'rejected');
    if (failures.length > 0) {
      setCatalogError(failures.map((one) => one.reason?.message || 'read failed').join(' '));
    }
  }

  function cancelEdit() {
    setEditing(false);
    setCatalogError(null);
  }

  async function save(id, body) {
    setBusy(true);
    clearMessages();
    try {
      const updated = await api.update(id, body);
      merge(updated);
      setEditing(false);
      setNotice(
        `Saved as revision ${updated.revision}. The reference below still names the last build; `
        + 'build again to get one for this revision.',
      );
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function build(id) {
    const alreadyBuilding = selected()?.state === 'building';
    setBusy(true);
    clearMessages();
    try {
      const outcome = await api.build(id);
      merge(outcome.recipe);
      if (outcome.reused) {
        setNotice(
          'Nothing changed since the last build: the existing image already matches this recipe, '
          + 'so no new build was started.',
        );
      } else if (alreadyBuilding) {
        setNotice(`A build is already running as job ${outcome.job_id}. This row changes when it ends.`);
      } else {
        setNotice(
          `Build queued as job ${outcome.job_id}. The row changes when the job ends; its log is under Operations.`,
        );
      }
      if (outcome.recipe.state === 'building') startPolling(id);
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  function startPolling(id) {
    stopPolling();
    polls = 0;
    timer = setInterval(async () => {
      polls += 1;
      try {
        const fresh = await api.get(id);
        merge(fresh);
        if (fresh.state === 'building' && polls < POLL_LIMIT) return;
        stopPolling();
        if (fresh.state === 'built') {
          setNotice(`Built. The image is ${fresh.image_ref}.`);
        } else if (fresh.state === 'failed') {
          setNotice('The build failed. The error is shown verbatim under the reference.');
        } else if (polls >= POLL_LIMIT) {
          setNotice(
            'Still building after two minutes. Reload to check again; closing this panel does not stop the job.',
          );
        }
      } catch {
        // A failed poll is not a failed build: stop asking and keep the row.
        stopPolling();
      }
    }, POLL_MS);
  }

  async function instantiate(id) {
    const name = sandboxName().trim();
    if (!name) {
      setError('A sandbox needs a name.');
      setErrorCode('sandbox.name_required');
      return;
    }
    setBusy(true);
    clearMessages();
    try {
      const sandbox = await api.instantiate(id, name);
      setNotice(`Sandbox “${sandbox.name}” created. Its containers and mounts are in Sandboxes.`);
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  async function destroy(recipe) {
    setBusy(true);
    clearMessages();
    try {
      await api.remove(recipe.id);
      close();
      setNotice(`Recipe “${recipe.name}” deleted. Sandboxes created from it keep running.`);
      await refresh();
    } catch (failure) {
      fail(failure);
    } finally {
      setBusy(false);
    }
  }

  return {
    items, loading, busy, error, errorCode, notice, selected, editing,
    sandboxName, setSandboxName,
    catalogs, catalogError,
    load, open, close, startEdit, cancelEdit, reloadCatalog,
    save, build, instantiate, destroy, nodeLabel,
  };
}

/** A suggestion the operator can overwrite; a recipe is a template. */
function defaultSandboxName(recipeName) {
  const stem = recipeName.slice(0, 60).replace(/-+$/, '');
  return `${stem}-1`;
}
