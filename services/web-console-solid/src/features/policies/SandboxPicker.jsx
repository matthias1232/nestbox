// Choose which sandboxes something is applied to: some of them, or all.
//
// Used to assign a policy to several sandboxes and to copy rules from one sandbox to
// others. Nothing is applied until the operator confirms; the confirm button says how
// many sandboxes it will touch.

import { For, Show, createSignal } from 'solid-js';

export default function SandboxPicker(props) {
  // props: title, hint, sandboxes, allowAll, confirmLabel, onConfirm(ids, all), onCancel
  const [chosen, setChosen] = createSignal(new Set());
  const [all, setAll] = createSignal(false);

  const toggle = (id) => {
    const next = new Set(chosen());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setChosen(next);
  };
  const count = () => (all() ? 'every sandbox' : `${chosen().size} sandbox${chosen().size === 1 ? '' : 'es'}`);

  return (
    <div class="editor sandbox-picker">
      <h3>{props.title}</h3>
      <p class="muted small">{props.hint}</p>
      <Show when={props.allowAll}>
        <label class="pf-field inline">
          <input type="checkbox" checked={all()} onChange={(e) => setAll(e.currentTarget.checked)} />
          <span>Every sandbox that exists now (a sandbox created later does not get it: put rules in a policy and assign that policy instead)</span>
        </label>
      </Show>
      <Show when={!all()}>
        <div class="picker-list">
          <For each={props.sandboxes} fallback={<p class="muted">There are no sandboxes.</p>}>
            {(item) => (
              <label class="picker-row">
                <input type="checkbox" checked={chosen().has(item.id)} onChange={() => toggle(item.id)} />
                <span class="mono">{item.name}</span>
                <span class="muted small">{item.pack_key} · {item.status}</span>
              </label>
            )}
          </For>
        </div>
      </Show>
      <div class="actions">
        <button class="primary" disabled={!all() && chosen().size === 0}
                onClick={() => props.onConfirm([...chosen()], all())}>
          {props.confirmLabel} — {count()}
        </button>
        <button class="secondary" onClick={props.onCancel}>Cancel</button>
      </div>
    </div>
  );
}
