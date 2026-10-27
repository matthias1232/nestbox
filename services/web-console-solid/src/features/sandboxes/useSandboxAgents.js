// State and calls for the Agents tab.
//
// Two refusals are ordinary answers here, not crashes: `agent.exists` means the
// instance name is taken, `agent.not_found` means the list on screen is stale.
// Both end in a sentence and a fresh read, because both are states an operator
// can act on. Everything else is an error.

import { createSignal } from 'solid-js';
import * as sandboxes from '../../api/sandboxes.js';
import * as packsApi from '../../api/packs.js';

const blank = {
  pack_key: '',
  instance: '',
  home_mode: 'shared',
  persistence: 'ephemeral',
  port: '',
};

export function useSandboxAgents() {
  const [agents, setAgents] = createSignal([]);
  const [entrypoint, setEntrypoint] = createSignal([]);
  const [packs, setPacks] = createSignal([]);
  const [form, setForm] = createSignal({ ...blank });
  // Per agent, whether its persistent home survives removal. True is the
  // default and the API default: destroying data takes a second decision.
  const [keep, setKeep] = createSignal({});
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);

  function setField(key, value) {
    setForm({ ...form(), [key]: value });
  }

  function choosePack(key) {
    const current = form();
    setForm({
      ...current,
      pack_key: key,
      // A pack key is already a slug, so it is a valid instance; the operator
      // may still type another one. Only the empty field is filled.
      instance: current.instance.trim() ? current.instance : key,
    });
  }

  function keeps(instance) {
    return keep()[instance] !== false;
  }

  function setKeeps(instance, value) {
    setKeep({ ...keep(), [instance]: value });
  }

  async function load(sandboxId) {
    if (!sandboxId) return;
    setLoading(true);
    setError(null);
    const [agentAnswer, packAnswer] = await Promise.allSettled([
      sandboxes.agents(sandboxId),
      packsApi.agentPacks(),
    ]);
    const problems = [];

    if (agentAnswer.status === 'fulfilled') {
      setAgents(agentAnswer.value.agents || []);
      setEntrypoint(agentAnswer.value.entrypoint || []);
    } else {
      problems.push(agentAnswer.reason.message);
    }

    if (packAnswer.status === 'fulfilled') {
      const items = packAnswer.value.items || [];
      setPacks(items);
      if (!form().pack_key && items.length > 0) choosePack(items[0].key);
    } else {
      problems.push(
        `The agent pack catalog could not be read (${packAnswer.reason.message}), so no new agent can be chosen.`,
      );
    }

    setError(problems.length > 0 ? problems.join(' ') : null);
    setLoading(false);
  }

  async function add(sandboxId) {
    const current = form();
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const agent = await sandboxes.addAgent(sandboxId, {
        pack_key: current.pack_key,
        instance: current.instance.trim(),
        home_mode: current.home_mode,
        persistence: current.persistence,
        port: current.port === '' ? undefined : Number(current.port),
      });
      // The stored entrypoint now lists the agent. A running container keeps the
      // command it was created with, so the sentence says that rather than
      // implying the agent is already up.
      setNotice(
        `Agent “${agent.instance}” added. The entrypoint starts it; a running container keeps the command it was created with.`,
      );
      setForm({ ...blank, pack_key: current.pack_key });
      await load(sandboxId);
      return true;
    } catch (failure) {
      if (failure.code === 'agent.exists') {
        setNotice(failure.message);
      } else {
        setError(failure.message);
      }
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function remove(sandboxId, instance) {
    const keepData = keeps(instance);
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await sandboxes.removeAgent(sandboxId, instance, keepData);
      setNotice(
        keepData
          ? `Agent “${instance}” removed. Its persistent home and its data stay on the node.`
          : `Agent “${instance}” removed, and its persistent home is no longer kept.`,
      );
      await load(sandboxId);
      return true;
    } catch (failure) {
      if (failure.code === 'agent.not_found') {
        const sentence = `${failure.message} The list was read again.`;
        await load(sandboxId);
        setNotice(sentence);
      } else {
        setError(failure.message);
      }
      return false;
    } finally {
      setBusy(false);
    }
  }

  return {
    agents, entrypoint, packs, form, loading, busy, error, notice,
    setField, choosePack, keeps, setKeeps, load, add, remove,
  };
}
