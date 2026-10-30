// Persistent terminal tabs. Each id names a tmux session inside this sandbox.
//
// The tab list lives on the **server**, not in this browser. That is the fix for
// the failure this file used to have: `localStorage` remembered the ids but
// nothing told the sandbox which shells were alive, so a reload could open a
// fresh shell in place of the application the operator had left running. The
// server also knows whether the image has tmux at all, and the tab strip says so
// instead of quietly starting a shell that cannot survive the next reload.
//
// Closing a tab explicitly ends only that terminal session.

import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import * as api from '../../api/sandboxes.js';
import '../../styles/deck.css';

// `crypto.randomUUID` only exists in a secure context. The appliance is also
// served over plain HTTP on a LAN address, where the property is undefined and
// calling it throws during render — which used to blank the whole sandbox view.
// The fallback keeps the same shape (`t-` plus 20 hex-ish characters) because
// the runtime accepts only ASCII alphanumerics, `_` and `-` as a tab id.
const randomToken = () => {
  const webCrypto = globalThis.crypto;
  if (typeof webCrypto?.randomUUID === 'function') return webCrypto.randomUUID();
  if (typeof webCrypto?.getRandomValues === 'function') {
    const bytes = webCrypto.getRandomValues(new Uint8Array(10));
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  }
  return `${Date.now().toString(16)}${Math.random().toString(16).slice(2, 12)}`.slice(0, 20);
};
const newId = () => `t-${randomToken().replaceAll('-', '').slice(0, 20)}`;
const freshTab = (index) => ({ id: newId(), label: `Terminal ${index}`, alive: false });

export default function TerminalTab(props) {
  let host;
  let socket;
  let term;
  let fit;
  let observer;
  const [tabs, setTabs] = createSignal([]);
  const [active, setActive] = createSignal(null);
  const [state, setState] = createSignal('connecting');
  const [note, setNote] = createSignal(null);
  const [durable, setDurable] = createSignal(null);
  const [loading, setLoading] = createSignal(true);

  const closeSocket = () => {
    try {
      socket?.close();
    } catch {
      /* gone */
    }
    socket = null;
  };

  function connect(tab = active()) {
    if (!tab) return;
    closeSocket();
    setState('connecting');
    setNote(null);
    term?.reset();
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    socket = new WebSocket(
      `${scheme}://${location.host}/api/v1/sandboxes/${props.sandbox.id}/terminal?tab=${encodeURIComponent(tab)}`,
      ['nestbox-terminal'],
    );
    socket.onopen = () => {
      setState('open');
      fit?.fit();
      socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      term.focus();
    };
    socket.onmessage = (event) => {
      const text = typeof event.data === 'string' ? event.data : '';
      if (text.startsWith('{')) {
        try {
          const frame = JSON.parse(text);
          if (frame?.type === 'close') {
            setNote(
              frame.message ||
                `Terminal detached (${frame.reason || 'closed'}). Reconnect attaches it again.`,
            );
            return;
          }
        } catch {
          /* output */
        }
      }
      term.write(text);
    };
    socket.onclose = () => setState('closed');
    socket.onerror = () =>
      setNote('The terminal could not be opened. The sandbox must be running and include the terminal runtime.');
  }

  /** Persist the set, then render what the server stored. */
  async function persist(next) {
    setTabs(next);
    try {
      const saved = await api.saveTerminalTabs(props.sandbox.id, next);
      setDurable(saved.durable ?? durable());
    } catch (failure) {
      setNote(`The tab list could not be saved: ${failure.message}`);
    }
  }

  /** Read the server's list and open the first tab's shell. */
  async function load() {
    setLoading(true);
    try {
      const answer = await api.terminalTabs(props.sandbox.id);
      setDurable(answer.durable ?? null);
      let items = (answer.items || []).map((item) => ({
        id: item.tab_id,
        label: item.title,
        alive: item.alive,
      }));
      if (!items.length) {
        // A sandbox with no tabs gets one, and it is stored, so the next reload
        // finds the same shell instead of inventing a second one.
        items = [freshTab(1)];
        await api.saveTerminalTabs(props.sandbox.id, items).catch(() => null);
      }
      setTabs(items);
      setActive(items[0].id);
      connect(items[0].id);
    } catch (failure) {
      setNote(`The tab list could not be read: ${failure.message}`);
      const fallback = [freshTab(1)];
      setTabs(fallback);
      setActive(fallback[0].id);
      connect(fallback[0].id);
    } finally {
      setLoading(false);
    }
  }

  function add() {
    const next = [...tabs(), freshTab(tabs().length + 1)];
    persist(next);
    setActive(next.at(-1).id);
    queueMicrotask(() => connect(next.at(-1).id));
  }

  function select(id) {
    if (id === active()) return;
    setActive(id);
    connect(id);
  }

  function closeTab(id) {
    const item = tabs().find((tab) => tab.id === id);
    if (!item) return;
    if (!window.confirm(`Close ${item.label}? Its shell and foreground command will end.`)) return;
    if (id === active() && socket?.readyState === WebSocket.OPEN) socket.send('exit\r');
    closeSocket();
    let next = tabs().filter((tab) => tab.id !== id);
    if (!next.length) next = [freshTab(1)];
    persist(next);
    setActive(next[0].id);
    queueMicrotask(() => connect(next[0].id));
  }

  onMount(() => {
    // A terminal that cannot attach must explain itself on its own tab instead
    // of taking the whole sandbox view down with it.
    try {
      term = new Terminal({
        cursorBlink: true,
        fontFamily: 'ui-monospace, Menlo, monospace',
        fontSize: 13,
        theme: { background: '#000000' },
      });
      fit = new FitAddon();
      term.loadAddon(fit);
      term.open(host);
      fit.fit();
      term.onData((data) => socket?.readyState === WebSocket.OPEN && socket.send(data));
      observer = new ResizeObserver(() => {
        fit.fit();
        if (socket?.readyState === WebSocket.OPEN) {
          socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
        }
      });
      observer.observe(host);
    } catch (failure) {
      setState('closed');
      setNote(`The terminal renderer could not start: ${failure?.message || failure}`);
      setLoading(false);
      return;
    }
    load();
  });

  onCleanup(() => {
    observer?.disconnect();
    closeSocket();
    term?.dispose();
  });

  return (
    <section class="tab-panel">
      <div class="controls">
        <Show when={!loading()} fallback={<span class="muted small">Reading the tabs…</span>}>
          <For each={tabs()}>
            {(tab) => (
              <span class={`terminal-tab ${active() === tab.id ? 'active' : ''}`}>
                <button class="tiny" onClick={() => select(tab.id)}>
                  {tab.label}
                  <Show when={tab.alive === false && tabs().length > 0}>
                    <span class="muted" title="The shell is not running; connecting starts it again">
                      {' '}· ended
                    </span>
                  </Show>
                </button>
                <button class="tiny danger" title="Close terminal" onClick={() => closeTab(tab.id)}>
                  ×
                </button>
              </span>
            )}
          </For>
          <button class="tiny" onClick={add}>+ Terminal</button>
        </Show>
      </div>
      <div class="controls">
        <span class={`badge ${state() === 'open' ? 'ok' : state() === 'connecting' ? 'warn' : 'muted'}`}>
          <i />
          {state()}
        </span>
        <Show when={state() === 'closed'}>
          <button class="primary tiny" onClick={() => connect()}>Reconnect</button>
        </Show>
        <span class="muted small">
          Tabs are stored on the server, so a reload or another machine reopens the same shells.
          Closing the browser detaches; × ends only that terminal.
        </span>
      </div>
      <Show when={durable() === false}>
        <p class="warn-note">
          This image has no <code>tmux</code>, so a tab cannot survive a reload: reconnecting starts a
          new shell. Recreate the sandbox from an image that includes it to keep applications
          running.
        </p>
      </Show>
      <Show when={note()}>
        <p class="notice">{note()}</p>
      </Show>
      <div class="term-wrap">
        <div ref={host} style="height:100%" />
      </div>
    </section>
  );
}
