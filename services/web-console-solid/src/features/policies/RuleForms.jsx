// Forms for a network rule and a DNS rule.
//
// A rule is a list of conditions (matchers) that all have to hold, an action and
// an enforcement. A rule can apply everywhere, to one policy, or to one sandbox;
// the scope picker says which and the body carries exactly one of them.

import { For, Show, createSignal } from 'solid-js';
import {
  MATCHER_KINDS, blankRuleForm, dnsProblem, dnsToPayload, ruleProblem, ruleToPayload,
} from './policyForms.js';
import * as api from '../../api/policies.js';
import { RULE_HELP, MATCHER_HELP, DNS_HELP } from './ruleHelp.js';
import '../../styles/policy-forms.css';

function ScopePicker(props) {
  // value: { kind: 'global'|'policy'|'sandbox', id }
  return (
    <>
      <label>
        Applies to
        <select value={props.value.kind} onChange={(e) => props.onChange({ kind: e.currentTarget.value, id: '' })}>
          <option value="policy">one policy (every sandbox that carries it)</option>
          <option value="sandbox">one sandbox</option>
        </select>
        <small class="help">{RULE_HELP.scope}</small>
      </label>
      <Show when={props.value.kind === 'policy'}>
        <label>
          Policy
          <select value={props.value.id} onChange={(e) => props.onChange({ ...props.value, id: e.currentTarget.value })}>
            <option value="">choose…</option>
            <For each={props.policies || []}>{(item) => <option value={item.id}>{item.name}</option>}</For>
          </select>
          <small class="help">{RULE_HELP.policy}</small>
        </label>
      </Show>
      <Show when={props.value.kind === 'sandbox'}>
        <label>
          Sandbox
          <select value={props.value.id} onChange={(e) => props.onChange({ ...props.value, id: e.currentTarget.value })}>
            <option value="">choose…</option>
            <For each={props.sandboxes || []}>{(item) => <option value={item.id}>{item.name}</option>}</For>
          </select>
          <small class="help">{RULE_HELP.sandbox}</small>
        </label>
      </Show>
    </>
  );
}

function scoped(form, scope) {
  return {
    ...form,
    policy_id: scope.kind === 'policy' ? scope.id : null,
    sandbox_id: scope.kind === 'sandbox' ? scope.id : null,
  };
}

function scopeProblem(scope) {
  // The server requires a target: a rule with none would apply to nothing.
  return !scope.id ? `Choose the ${scope.kind} this rule applies to.` : null;
}

export function NetworkRuleForm(props) {
  const [form, setForm] = createSignal({ ...blankRuleForm(), ...(props.initial || {}) });
  const [scope, setScope] = createSignal(
    props.sandboxId ? { kind: 'sandbox', id: props.sandboxId } : { kind: 'policy', id: '' },
  );
  const [problem, setProblem] = createSignal(null);
  const [busy, setBusy] = createSignal(false);
  const set = (patch) => setForm({ ...form(), ...patch });
  const setMatcher = (index, patch) =>
    set({ matchers: form().matchers.map((m, i) => (i === index ? { ...m, ...patch } : m)) });

  async function submit(event) {
    event.preventDefault();
    const issue = ruleProblem(form()) || scopeProblem(scope());
    setProblem(issue);
    if (issue) return;
    setBusy(true);
    try {
      await api.createRule(ruleToPayload(scoped(form(), scope())));
      setForm(blankRuleForm());
      props.onCreated?.();
    } catch (failure) {
      setProblem(failure.message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <form class="rule-form" onSubmit={submit}>
      <h3>New network rule</h3>
      <div class="row">
        <label>Name<input value={form().name} onInput={(e) => set({ name: e.currentTarget.value })} placeholder="allow npm" />
          <small class="help">{RULE_HELP.name}</small></label>
        <label>Action
          <select value={form().action} onChange={(e) => set({ action: e.currentTarget.value })}>
            <option value="allow">allow</option><option value="deny">deny</option>
          </select>
          <small class="help">{RULE_HELP.action}</small>
        </label>
        <label>Enforcement
          <select value={form().enforcement} onChange={(e) => set({ enforcement: e.currentTarget.value })}>
            <option value="enforced">enforced (blocks)</option><option value="audit">audit (records, allows)</option>
          </select>
          <small class="help">{RULE_HELP.enforcement}</small>
        </label>
        <label>Protocol
          <select value={form().protocol} onChange={(e) => set({ protocol: e.currentTarget.value })}>
            <option value="">any protocol</option>
            <For each={['rest', 'http', 'websocket', 'graphql', 'mcp', 'jsonrpc', 'tcp']}>{(p) => <option value={p}>{p}</option>}</For>
          </select>
          <small class="help">{RULE_HELP.protocol}</small>
        </label>
        <label>Priority<input type="number" value={form().priority} onInput={(e) => set({ priority: e.currentTarget.value })} style="width:80px" />
          <small class="help">{RULE_HELP.priority}</small></label>
      </div>

      <div>
        <strong class="small">Conditions</strong>
        <span class="muted small"> · all of them must match; for several methods or hosts, create one rule each</span>
        <p class="help">{RULE_HELP.conditions}</p>
        <For each={form().matchers}>
          {(matcher, index) => (
            <div class="matcher-row">
              <select value={matcher.kind} onChange={(e) => setMatcher(index(), { kind: e.currentTarget.value })}>
                <For each={MATCHER_KINDS}>{(k) => <option value={k.kind}>{k.label}</option>}</For>
              </select>
              <Show when={matcher.kind !== 'any'}>
                <input
                  value={matcher.value ?? ''}
                  placeholder={MATCHER_KINDS.find((k) => k.kind === matcher.kind)?.placeholder}
                  onInput={(e) => setMatcher(index(), { value: e.currentTarget.value })}
                />
              </Show>
              <button type="button" class="tiny" onClick={() => set({ matchers: form().matchers.filter((_, i) => i !== index()) })}>Remove</button>
              <small class="help">{MATCHER_HELP[matcher.kind]}</small>
            </div>
          )}
        </For>
        <button type="button" class="secondary tiny" onClick={() => set({ matchers: [...form().matchers, { kind: 'host', value: '' }] })}>Add condition</button>
      </div>

      <div class="row">
        <label>HTTP method (optional)<input value={form().method} onInput={(e) => set({ method: e.currentTarget.value })} placeholder="GET" />
          <small class="help">{RULE_HELP.method}</small></label>
        <label>Executable (optional)<input value={form().binary} onInput={(e) => set({ binary: e.currentTarget.value })} placeholder="/usr/bin/curl" />
          <small class="help">{RULE_HELP.binary}</small></label>
        <ScopePicker value={scope()} onChange={setScope} policies={props.policies} sandboxes={props.sandboxes} />
      </div>

      <Show when={problem()}><p class="error">{problem()}</p></Show>
      <div class="controls"><button class="primary" type="submit" disabled={busy()}>Create rule</button></div>
    </form>
  );
}

export function DnsRuleForm(props) {
  const [form, setForm] = createSignal({ action: 'deny', pattern: '', priority: 100 });
  const [scope, setScope] = createSignal(
    props.sandboxId ? { kind: 'sandbox', id: props.sandboxId } : { kind: 'policy', id: '' },
  );
  const [problem, setProblem] = createSignal(null);
  const set = (patch) => setForm({ ...form(), ...patch });

  async function submit(event) {
    event.preventDefault();
    const issue = dnsProblem(form()) || scopeProblem(scope());
    setProblem(issue);
    if (issue) return;
    try {
      await api.createDnsRule(dnsToPayload(scoped(form(), scope())));
      setForm({ action: form().action, pattern: '', priority: 100 });
      props.onCreated?.();
    } catch (failure) {
      setProblem(failure.message);
    }
  }

  return (
    <form class="rule-form" onSubmit={submit}>
      <h3>New DNS rule</h3>
      <div class="row">
        <label>Action
          <select value={form().action} onChange={(e) => set({ action: e.currentTarget.value })}>
            <option value="deny">deny (block the name)</option><option value="allow">allow</option>
          </select>
          <small class="help">{DNS_HELP.action}</small>
        </label>
        <label>Name or .suffix<input value={form().pattern} onInput={(e) => set({ pattern: e.currentTarget.value })} placeholder=".tracker.example" />
          <small class="help">{DNS_HELP.pattern}</small></label>
        <label>Priority<input type="number" value={form().priority} onInput={(e) => set({ priority: e.currentTarget.value })} style="width:80px" />
          <small class="help">{DNS_HELP.priority}</small></label>
        <ScopePicker value={scope()} onChange={setScope} policies={props.policies} sandboxes={props.sandboxes} />
      </div>
      <Show when={problem()}><p class="error">{problem()}</p></Show>
      <div class="controls"><button class="primary" type="submit">Create DNS rule</button></div>
    </form>
  );
}
