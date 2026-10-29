// Policies, network rules, and the draft queue.

import { For, Show, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import Tabs from '../../components/Tabs.jsx';
import * as api from '../../api/policies.js';
import * as sandboxApi from '../../api/sandboxes.js';
import PolicyForm from './PolicyForm.jsx';
import { DnsRuleForm, NetworkRuleForm } from './RuleForms.jsx';
import { blankSpec } from './policyForms.js';

const TABS = [
  { id: 'policies', label: 'Policies' },
  { id: 'rules', label: 'Network rules' },
  { id: 'dns', label: 'DNS rules' },
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
  const [schema, setSchema] = createSignal(null);
  const [templates, setTemplates] = createSignal([]);
  const [dnsRules, setDnsRules] = createSignal([]);
  const [sandboxes, setSandboxes] = createSignal([]);
  const [mode, setMode] = createSignal('form'); // form | json
  const [spec, setSpec] = createSignal(null);
  const [creating, setCreating] = createSignal(false);
  const [draft, setDraft] = createSignal({ name: '', scope: 'sandbox', template: '', spec: blankSpec() });

  onMount(load);

  async function load() {
    setError(null);
    try {
      const [policyList, ruleList, draftList, dnsList, schemaDoc, templateList, sandboxList] = await Promise.all([
        api.list(), api.networkRules(), api.drafts(), api.dnsRules(), api.schema(), api.templates(),
        sandboxApi.list({ limit: 200 }),
      ]);
      setDnsRules(dnsList.items || []);
      setSchema(schemaDoc);
      setTemplates(templateList.items || []);
      setSandboxes((sandboxList.items || []).map((row) => row.sandbox || row));
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
    setCreating(false);
    setSpec(policy.spec);
    setEditor(JSON.stringify(policy.spec, null, 2));
    setOutcome(null);
  }

  // Start a new policy from a blank document or from a preset.
  function startCreate() {
    setSelected(null);
    setOutcome(null);
    setCreating(true);
    setDraft({ name: '', scope: 'sandbox', template: '', spec: blankSpec() });
  }

  function chooseTemplate(id) {
    const found = templates().find((item) => item.id === id);
    setDraft({ ...draft(), template: id, spec: found ? structuredClone(found.spec) : blankSpec() });
  }

  async function create() {
    setError(null);
    try {
      const body = { name: draft().name.trim(), scope: draft().scope, spec: mode() === 'json' ? JSON.parse(editor()) : draft().spec };
      if (!body.name) throw new Error('Give the policy a name.');
      const made = await api.create(body);
      setCreating(false);
      await load();
      open(made.policy || made);
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function removeSelected() {
    if (!window.confirm(`Delete the policy “${selected().name}”? Sandboxes that carry it lose their rules.`)) return;
    setError(null);
    try {
      await api.remove(selected().id);
      setSelected(null);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function save() {
    setError(null);
    setOutcome(null);
    try {
      const document = mode() === 'json' ? JSON.parse(editor()) : spec();
      const result = await api.update(selected().id, { spec: document, version: selected().version });
      setOutcome(result);
      await load();
      const refreshed = (await api.get(selected().id));
      setSelected(refreshed);
      setSpec(refreshed.spec);
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
          <button class="primary" onClick={startCreate}>New policy</button>
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

          <Show when={creating()}>
            <div class="editor">
              <h3>New policy</h3>
              <div class="pf-fields">
                <label class="pf-field"><span>Name</span>
                  <input class="pf-input" value={draft().name} onInput={(e) => setDraft({ ...draft(), name: e.currentTarget.value })} />
                </label>
                <label class="pf-field"><span>Scope</span>
                  <select class="pf-input" value={draft().scope} onChange={(e) => setDraft({ ...draft(), scope: e.currentTarget.value })}>
                    <option value="sandbox">sandbox (assign it to sandboxes)</option>
                    <option value="global">global override (replaces every sandbox policy)</option>
                  </select>
                </label>
                <label class="pf-field"><span>Start from</span>
                  <select class="pf-input" value={draft().template} onChange={(e) => chooseTemplate(e.currentTarget.value)}>
                    <option value="">blank document</option>
                    <For each={templates()}>{(item) => <option value={item.id}>{item.name}</option>}</For>
                  </select>
                </label>
              </div>
              <div class="controls">
                <button class={mode() === 'form' ? 'primary tiny' : 'secondary tiny'} onClick={() => setMode('form')}>Form</button>
                <button class={mode() === 'json' ? 'primary tiny' : 'secondary tiny'}
                  onClick={() => { setEditor(JSON.stringify(draft().spec, null, 2)); setMode('json'); }}>JSON</button>
              </div>
              <Show when={mode() === 'form'} fallback={
                <textarea value={editor()} onInput={(e) => setEditor(e.currentTarget.value)} spellcheck={false} rows="18" />}>
                <PolicyForm schema={schema()} spec={draft().spec} onChange={(next) => setDraft({ ...draft(), spec: next })} />
              </Show>
              <div class="actions">
                <button class="primary" onClick={create}>Create policy</button>
                <button class="secondary" onClick={() => setCreating(false)}>Cancel</button>
              </div>
            </div>
          </Show>

          <Show when={selected() && !creating()}>
            <div class="editor">
              <h3>{selected().name}</h3>
              <p class="muted small">
                Four sections: filesystem, Landlock, process, network. Only the
                network section can change under a running workload.
              </p>
              <div class="controls">
                <button class={mode() === 'form' ? 'primary tiny' : 'secondary tiny'}
                  onClick={() => { try { setSpec(JSON.parse(editor())); setMode('form'); } catch (e) { setError('The JSON is not valid: ' + e.message); } }}>Form</button>
                <button class={mode() === 'json' ? 'primary tiny' : 'secondary tiny'}
                  onClick={() => { setEditor(JSON.stringify(spec(), null, 2)); setMode('json'); }}>JSON</button>
              </div>
              <Show when={mode() === 'form'} fallback={
                <textarea value={editor()} onInput={(e) => setEditor(e.currentTarget.value)} spellcheck={false} rows="18" />}>
                <PolicyForm schema={schema()} spec={spec()} onChange={setSpec} />
              </Show>
              <div class="actions">
                <button onClick={save}>Validate and save</button>
                <button class="secondary" onClick={removeSelected}>Delete policy</button>
              </div>
              <Show when={outcome()}>
                <p class={outcome().outcome === 'applies_now' ? 'notice' : 'warn-note'}>
                  {SECTION_NOTE[outcome().outcome]}
                  {' '}Sections: {outcome().sections.join(', ') || 'none'}.
                </p>
              </Show>
            </div>
          </Show>
          <Show when={!selected() && !creating()}>
            <p class="muted">Select a policy to edit it, or create a new one.</p>
          </Show>
        </div>
      </Show>

      <Show when={tab() === 'rules'}>
        <NetworkRuleForm policies={policies()} sandboxes={sandboxes()} onCreated={load} />
        <Show when={rules().length > 0} fallback={<p class="muted">No network rules yet.</p>}>
          <div class="table-scroll">
            <table>
              <thead>
                <tr><th>Action</th><th>Host</th><th class="narrow">Port</th>
                    <th>Protocol</th><th>Enforcement</th><th>Binaries</th><th /></tr>
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
                      <td><button class="tiny" onClick={async () => { await api.removeRule(rule.id); load(); }}>Delete</button></td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>

      <Show when={tab() === 'dns'}>
        <DnsRuleForm policies={policies()} sandboxes={sandboxes()} onCreated={load} />
        <Show when={dnsRules().length > 0} fallback={<p class="muted">No DNS rules yet. Without one, every name a sandbox asks for is resolved.</p>}>
          <div class="table-scroll">
            <table>
              <thead><tr><th>Action</th><th>Pattern</th><th class="narrow">Priority</th><th /></tr></thead>
              <tbody>
                <For each={dnsRules()}>
                  {(rule) => (
                    <tr>
                      <td><StatusBadge status={rule.action === 'allow' ? 'running' : 'denied'} label={rule.action} /></td>
                      <td class="mono small">{rule.pattern}</td>
                      <td class="narrow mono">{rule.priority}</td>
                      <td><button class="tiny" onClick={async () => { await api.removeDnsRule(rule.id); load(); }}>Delete</button></td>
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
