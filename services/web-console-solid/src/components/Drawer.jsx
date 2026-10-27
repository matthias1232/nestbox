// A side panel.
//
// Built as a plain overlay rather than a <dialog> so it works identically in the
// desktop shell and in a narrow browser, and so the caller keeps control of
// what is rendered inside.

import { Show } from 'solid-js';

export default function Drawer(props) {
  return (
    <Show when={props.open}>
      <div class="drawer-backdrop" onClick={props.onClose} />
      <aside class="drawer" role="dialog" aria-label={props.title}>
        <header>
          <div>
            <h2>{props.title}</h2>
            <Show when={props.subtitle}>
              <p class="muted">{props.subtitle}</p>
            </Show>
          </div>
          <button class="ghost" onClick={props.onClose} aria-label="Close">×</button>
        </header>
        <div class="drawer-body">{props.children}</div>
      </aside>
    </Show>
  );
}
