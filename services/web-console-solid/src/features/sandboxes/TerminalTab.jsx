// A real terminal in the browser: xterm.js over the terminal WebSocket.
//
// Wire protocol (fixed, see the runner's terminal.py): a text frame from the
// browser is keyboard input, except one JSON control frame, {"type":"resize",
// "cols","rows"}; a text frame from the server is output; before it closes, the
// server sends {"type":"close","reason": idle | user | sandbox_stopped | error}.
// The session cookie authenticates the upgrade; nothing typed or printed is
// recorded anywhere (ADR 0023).
import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import '../../styles/deck.css';

const REASONS = {
  idle: 'The session was closed because it was idle.',
  user: 'The session ended.',
  sandbox_stopped: 'The sandbox stopped.',
  error: 'The session ended with an error.',
};

export default function TerminalTab(props) {
  let host;
  let socket = null;
  let term = null;
  let fit = null;
  const [state, setState] = createSignal('connecting');
  const [note, setNote] = createSignal(null);

  function connect() {
    setNote(null);
    setState('connecting');
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    socket = new WebSocket(`${scheme}://${location.host}/api/v1/sandboxes/${props.sandbox.id}/terminal`, ['nestbox-terminal']);
    socket.onopen = () => {
      setState('open');
      fit.fit();
      socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      term.focus();
    };
    socket.onmessage = (event) => {
      const text = typeof event.data === 'string' ? event.data : '';
      if (text.startsWith('{')) {
        try {
          const frame = JSON.parse(text);
          if (frame && frame.type === 'close') {
            setNote(frame.message || REASONS[frame.reason] || 'The session ended.');
            return;
          }
        } catch { /* plain output that happens to start with a brace */ }
      }
      term.write(text);
    };
    socket.onclose = () => setState('closed');
    socket.onerror = () => setNote('The terminal could not be opened. The sandbox must be running, and you need the shell permission.');
  }

  onMount(() => {
    term = new Terminal({ cursorBlink: true, fontFamily: 'ui-monospace, Menlo, monospace', fontSize: 13, theme: { background: '#000000' } });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();
    term.onData((data) => socket?.readyState === WebSocket.OPEN && socket.send(data));
    const observer = new ResizeObserver(() => {
      fit.fit();
      if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
    });
    observer.observe(host);
    onCleanup(() => observer.disconnect());
    connect();
  });

  onCleanup(() => {
    try { socket?.close(); } catch { /* already closed */ }
    term?.dispose();
  });

  return (
    <section class="tab-panel">
      <div class="controls">
        <span class={`badge ${state() === 'open' ? 'ok' : state() === 'connecting' ? 'warn' : 'muted'}`}><i />{state()}</span>
        <Show when={state() === 'closed'}>
          <button class="primary tiny" onClick={() => { term.reset(); connect(); }}>Reconnect</button>
        </Show>
        <span class="muted small">Not a fit for your SSH client? The Access tab shows how to connect with ssh or PuTTY.</span>
      </div>
      <Show when={note()}><p class="notice">{note()}</p></Show>
      <div class="term-wrap"><div ref={host} style="height:100%" /></div>
    </section>
  );
}
