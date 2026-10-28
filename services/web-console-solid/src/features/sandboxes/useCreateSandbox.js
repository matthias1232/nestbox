// State and calls for the create form.
//
// The node's budget is the server's answer to `GET /nodes`; nothing here sums
// installs or guesses a total. The one comparison made in this file is a
// courtesy: it turns "8 GiB" into bytes and asks whether that is more than the
// free bytes the server reported, so the form can refuse before the request
// instead of after it. The request still goes to the server whenever it is sent,
// and the server stays the authority.

import { createMemo, createSignal, onMount } from 'solid-js';
import { gib } from '../../components/ResourceBars.jsx';
import * as nodesApi from '../../api/nodes.js';
import * as packsApi from '../../api/packs.js';
import * as sandboxes from '../../api/sandboxes.js';

const MIB = 1024 * 1024;
const MIB_PER_GIB = 1024;

// The API's own defaults, shown as the starting point rather than an opinion.
const blank = {
  name: '',
  pack_key: '',
  node: '',
  memory_gib: '2',
  cpu: '1',
  pids: '512',
};

export function useCreateSandbox() {
  const [nodes, setNodes] = createSignal([]);
  const [packs, setPacks] = createSignal([]);
  const [form, setForm] = createSignal({ ...blank });
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);

  onMount(load);

  async function load() {
    setLoading(true);
    setError(null);
    const [nodeAnswer, packAnswer] = await Promise.allSettled([
      nodesApi.list(),
      packsApi.agentPacks(),
    ]);
    const problems = [];

    if (nodeAnswer.status === 'fulfilled') {
      // Default to the local node, then to the first one: the same placement a
      // request without `node` gets, so the visible choice matches the actual
      // one.
      const items = nodeAnswer.value.items || [];
      setNodes(items);
      const preferred = items.find((node) => node.is_local) || items[0];
      if (preferred) setForm({ ...form(), node: preferred.name });
    } else {
      problems.push(
        `The node list could not be read (${nodeAnswer.reason.message}). A request without a node is placed on the local node.`,
      );
    }

    if (packAnswer.status === 'fulfilled') {
      const items = packAnswer.value.items || [];
      setPacks(items);
      if (!form().pack_key && items.length > 0) setForm({ ...form(), pack_key: items[0].key });
    } else {
      problems.push(`The agent packs could not be read (${packAnswer.reason.message}).`);
    }

    setError(problems.length > 0 ? problems.join(' ') : null);
    setLoading(false);
  }

  function setField(key, value) {
    setForm({ ...form(), [key]: value });
  }

  const selectedNode = createMemo(
    () => nodes().find((node) => node.name === form().node) || null,
  );

  // `hardware_known` is the node's own statement that someone measured it. A
  // node nobody measured reports a zero budget, and treating that zero as "full"
  // would refuse every request on a machine that is probably empty.
  const measured = createMemo(() => selectedNode()?.hardware_known === true);
  const budget = createMemo(() => selectedNode()?.budget || null);

  const memoryBytes = createMemo(() => {
    const value = Number(form().memory_gib);
    if (!Number.isFinite(value) || value <= 0) return null;
    return Math.round(value * MIB_PER_GIB) * MIB;
  });

  const cpuCores = createMemo(() => {
    const value = Number(form().cpu);
    return Number.isFinite(value) && value > 0 ? value : null;
  });

  const pids = createMemo(() => {
    const value = Number(form().pids);
    return Number.isInteger(value) && value > 0 ? value : null;
  });

  // Every reason the form will not submit, in the order the fields are read.
  // Each sentence carries the number it is about; "does not fit" alone would
  // make an operator guess which field to change.
  const problems = createMemo(() => {
    const current = form();
    const found = [];
    if (!current.name.trim()) found.push('A sandbox name is required.');
    if (!current.pack_key) found.push('An agent pack is required.');
    if (memoryBytes() === null) found.push('Memory is a positive number of GiB.');
    if (cpuCores() === null) found.push('CPU is a positive number of cores.');
    if (pids() === null) found.push('PIDs is a positive whole number.');

    const room = budget();
    if (measured() && room) {
      if (memoryBytes() !== null && memoryBytes() > room.ram_free_bytes) {
        found.push(
          `The node has ${gib(room.ram_free_bytes)} free; ${gib(memoryBytes())} was asked for.`,
        );
      }
      if (cpuCores() !== null && room.cpu_cores > 0 && cpuCores() > room.cpu_cores) {
        found.push(
          `The node has ${room.cpu_cores} cores; ${cpuCores()} was asked for.`,
        );
      }
    }
    return found;
  });

  const blocked = createMemo(() => problems().length > 0);

  async function submit() {
    if (blocked() || busy()) return null;
    const current = form();
    setBusy(true);
    setError(null);
    try {
      const created = await sandboxes.create({
        name: current.name.trim(),
        pack_key: current.pack_key,
        // MiB keeps the request exact for a half GiB and avoids the whole-unit
        // rule `normalize_memory_limit` applies to the value it receives.
        memory_limit: `${Math.round(Number(current.memory_gib) * MIB_PER_GIB)}Mi`,
        cpu_limit: cpuCores(),
        pids_limit: pids(),
        ...(current.node ? { node: current.node } : {}),
      });
      return created;
    } catch (failure) {
      // Two refusals are about the node rather than the request's shape, and the
      // server's sentence already names the node and the number that failed.
      if (failure.code === 'node.insufficient_resources') {
        setError(`The node refused it: ${failure.message}`);
      } else if (failure.code === 'node.not_found') {
        // The name on screen is stale. Reading the list again replaces it; the
        // sentence survives the reload because that is the part to act on.
        const sentence = `${failure.message} The node list was read again; choose a node that exists.`;
        await load();
        setError(sentence);
      } else {
        setError(failure.message);
      }
      return null;
    } finally {
      setBusy(false);
    }
  }

  return {
    nodes, packs, form, loading, busy, error,
    setField, submit,
    selectedNode, measured, budget, memoryBytes, cpuCores, problems, blocked, load,
  };
}
