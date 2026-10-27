// Policies, network rules, and the draft queue.

import { For, Show, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import Tabs from '../../components/Tabs.jsx';
import * as api from '../../api/policies.js';

const TABS = [
  { id: 'policies', label: 'Policies' },
  { id: 'rules', label: 'Network rules' },
  { id: 'drafts', label: 'Pending drafts' },
];

const SECTION_NOTE = {
  applies_now: 'Applied. Running sandboxes reload the network section and reopen connections.',
  requires_recreate: 'Saved. Filesystem, Landlock, or process changes need the sandbox to be recreated before they take effect.',
  rejected: 'Refused by the runtime.',
};

export default function PoliciesPage() {
  const [tab, setTab] = createSignal('policies');
  const [policies, setPolicies] = createSignal([]);
  const [globalPolicy, setGlobalPolicy] = createSignal(null);
  const [rules, setRules] = createSignal([]);
  const [drafts, setDrafts] = createSignal([]);
  const [selected, setSelected] = createSignal(null);
  const [editor, setEditor] = createSignal('');
  const [error, setError] = createSignal(null);
  const [outcome, setOutcome] = createSignal(null);

  onMount(load);

  async function load() {
    setError(null);
    try {
      const [policyList, ruleList, draftList] = await Promise.all([
        api.list(), api.networkRules(), api.drafts(),
      ]);
      setPolicies(policyList.items || []);
      setGlobalPolicy(policyList.global || null);
      setRules(ruleList.items || []);
      setDrafts(draftList.items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }

  function open(policy) {
    setSelected(policy);
    setEditor(JSON.stringify(policy.spec, null, 2));
    setOutcome(null);
  }

  async function save() {
    setError(null);
    setOutcome(null);
    try {
      const spec = JSON.parse(editor());
      const result = await api.update(selected().id, { spec, version: selected().version });
      setOutcome(result);
      await load();
      const refreshed = (await api.get(selected().id));
      setSelected(refreshed);
      setEditor(JSON.stringify(refreshed.spec, null, 2));
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function decide(draft, approve) {
    setError(null);
    try {
      await api.decideDraft(draft.id, approve, approve ? null : 'rejected in console');
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Policies</h1>
          <p class="muted">
            {policies().length} policies · {rules().length} rules · {drafts().length} drafts pending
          </p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={load}>Refresh</button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>

      <Show when={globalPolicy()}>
        <p class="notice">
          A global override is active (“{globalPolicy().name}”). It replaces every
          sandbox policy and blocks sandbox-level edits until it is deleted.
        </p>
      </Show>

      <Tabs tabs={TABS} active={tab()} onChange={setTab} />

      <Show when={tab() === 'policies'}>
        <div class="split">
          <div class="table-scroll">
            <table>
              <thead><tr><th>Name</th><th>Scope</th><th class="narrow">Version</th><th>Updated</th></tr></thead>
              <tbody>
                <For each={policies()}>
                  {(policy) => (
                    <tr class={selected()?.id === policy.id ? 'selected' : ''}
                        onClick={() => open(policy)}>
                      <td>{policy.name}</td>
                      <td><StatusBadge status={policy.scope === 'global' ? 'degraded' : 'running'}
                                       label={policy.scope} /></td>
                      <td class="narrow mono">{policy.version}</td>
                      <td class="mono small">{policy.updated_at}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>

          <Show when={selected()} fallback={<p class="muted">Select a policy to edit it.</p>}>
            <div class="editor">
              <h3>{selected().name}</h3>
              <p class="muted small">
                Four sections: filesystem, Landlock, process, network. Only the
                network section can change under a running workload.
              </p>
              <textarea value={editor()} onInput={(e) => setEditor(e.currentTarget.value)}
                        spellcheck={false} rows="18" />
              <div class="actions">
                <button onClick={save}>Validate and save</button>
              </div>
              <Show when={outcome()}>
                <p class={outcome().outcome === 'applies_now' ? 'notice' : 'warn-note'}>
                  {SECTION_NOTE[outcome().outcome]}
                  {' '}Sections: {outcome().sections.join(', ') || 'none'}.
                </p>
              </Show>
            </div>
          </Show>
        </div>
      </Show>

      <Show when={tab() === 'rules'}>
        <Show when={rules().length > 0} fallback={<p class="muted">No network rules yet.</p>}>
          <div class="table-scroll">
            <table>
              <thead>
                <tr><th>Action</th><th>Host</th><th class="narrow">Port</th>
                    <th>Protocol</th><th>Enforcement</th><th>Binaries</th></tr>
              </thead>
              <tbody>
                <For each={rules()}>
                  {(rule) => (
                    <tr>
                      <td><StatusBadge status={rule.action === 'allow' ? 'running' : 'denied'} label={rule.action} /></td>
                      <td class="mono small">{rule.host}</td>
                      <td class="narrow mono">{rule.port ?? '—'}</td>
                      <td class="small">{rule.protocol}</td>
                      <td class="small">{rule.enforcement}</td>
                      <td class="mono small">{rule.binaries.join(', ') || 'any'}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === 'drafts'}>
        <Show when={drafts().length > 0} fallback={<p class="muted">No pending drafts. Create one from the live traffic console.</p>}>
          <For each={drafts()}>
            {(draft) => (
              <div class="draft-card">
                <div class="draft-head">
                  <strong>{draft.draft?.host || 'draft'}</strong>
                  <span class="muted small">{draft.source_kind} · {draft.target_type}</span>
                </div>
                <pre>{JSON.stringify(draft.draft, null, 2)}</pre>
                <div class="actions">
                  <button onClick={() => decide(draft, true)}>Approve and enforce</button>
                  <button class="secondary" onClick={() => decide(draft, false)}>Reject</button>
                </div>
              </div>
            )}
          </For>
        </Show>
      </Show>
    </section>
  );
}
