// One terminal in its own browser window.
//
// The shell lives on the server (`tmux`), so closing this window does not end it:
// reopening the same tab attaches again, with the history and anything still
// running. Everything anyone types into the session is shared, which is the point
// of opening it next to someone else.
import { Show, createSignal, onMount } from 'solid-js';
import * as api from '../../api/sandboxes.js';
import TerminalTab from './TerminalTab.jsx';

export default function TerminalWindow(props) {
  const [sandbox, setSandbox] = createSignal(null);
  const [error, setError] = createSignal(null);

  onMount(async () => {
    try {
      const found = await api.get(props.sandboxId);
      setSandbox(found.sandbox || found);
      document.title = `Terminal · ${(found.sandbox || found).name}`;
    } catch (failure) {
      setError(failure.message);
    }
  });

  return (
    <div style="padding:.75rem">
      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={sandbox()} fallback={<p class="muted">Opening the terminal…</p>}>
        <TerminalTab sandbox={sandbox()} initialTab={props.tabId} popout />
      </Show>
    </div>
  );
}
