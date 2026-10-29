// Persistent terminal tabs. Each id names a tmux session inside this sandbox.
// Browser reloads recover tabs from localStorage and reattach instead of making
// a fresh shell. Closing a tab explicitly ends only that terminal session.
import { For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import '../../styles/deck.css';

const keyFor = (id) => `nestbox:terminal-tabs:${id}`;
const newId = () => `t-${crypto.randomUUID().replaceAll('-', '').slice(0, 20)}`;
const loadTabs = (id) => { try { const value = JSON.parse(localStorage.getItem(keyFor(id)) || '[]'); return Array.isArray(value) && value.length ? value : [{ id: newId(), label: 'Terminal 1' }]; } catch { return [{ id: newId(), label: 'Terminal 1' }]; } };

export default function TerminalTab(props) {
  let host; let socket; let term; let fit; let observer;
  const [tabs, setTabs] = createSignal(loadTabs(props.sandbox.id));
  const [active, setActive] = createSignal(tabs()[0].id);
  const [state, setState] = createSignal('connecting');
  const [note, setNote] = createSignal(null);
  const save = (next) => { setTabs(next); localStorage.setItem(keyFor(props.sandbox.id), JSON.stringify(next)); };
  const closeSocket = () => { try { socket?.close(); } catch { /* gone */ } socket = null; };
  function connect(tab = active()) {
    closeSocket(); setState('connecting'); setNote(null); term?.reset();
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    socket = new WebSocket(`${scheme}://${location.host}/api/v1/sandboxes/${props.sandbox.id}/terminal?tab=${encodeURIComponent(tab)}`, ['nestbox-terminal']);
    socket.onopen = () => { setState('open'); fit?.fit(); socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows })); term.focus(); };
    socket.onmessage = (event) => { const text = typeof event.data === 'string' ? event.data : ''; if (text.startsWith('{')) { try { const frame = JSON.parse(text); if (frame?.type === 'close') { setNote(frame.message || `Terminal detached (${frame.reason || 'closed'}). Reconnect attaches it again.`); return; } } catch { /* output */ } } term.write(text); };
    socket.onclose = () => setState('closed');
    socket.onerror = () => setNote('The terminal could not be opened. The sandbox must be running and include the terminal runtime.');
  }
  function add() { const next = [...tabs(), { id: newId(), label: `Terminal ${tabs().length + 1}` }]; save(next); setActive(next.at(-1).id); queueMicrotask(() => connect(next.at(-1).id)); }
  function select(id) { if (id === active()) return; setActive(id); connect(id); }
  function closeTab(id) { const item = tabs().find((tab) => tab.id === id); if (!item || !window.confirm(`Close ${item.label}? Its shell and foreground command will end.`)) return; if (id === active() && socket?.readyState === WebSocket.OPEN) socket.send('exit\r'); closeSocket(); let next = tabs().filter((tab) => tab.id !== id); if (!next.length) next = [{ id: newId(), label: 'Terminal 1' }]; save(next); setActive(next[0].id); queueMicrotask(() => connect(next[0].id)); }
  onMount(() => { term = new Terminal({ cursorBlink: true, fontFamily: 'ui-monospace, Menlo, monospace', fontSize: 13, theme: { background: '#000000' } }); fit = new FitAddon(); term.loadAddon(fit); term.open(host); fit.fit(); term.onData((data) => socket?.readyState === WebSocket.OPEN && socket.send(data)); observer = new ResizeObserver(() => { fit.fit(); if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows })); }); observer.observe(host); connect(); });
  onCleanup(() => { observer?.disconnect(); closeSocket(); term?.dispose(); });
  return <section class="tab-panel"><div class="controls"><For each={tabs()}>{(tab) => <span class={`terminal-tab ${active() === tab.id ? 'active' : ''}`}><button class="tiny" onClick={() => select(tab.id)}>{tab.label}</button><button class="tiny danger" title="Close terminal" onClick={() => closeTab(tab.id)}>×</button></span>}</For><button class="tiny" onClick={add}>+ Terminal</button></div><div class="controls"><span class={`badge ${state() === 'open' ? 'ok' : state() === 'connecting' ? 'warn' : 'muted'}`}><i />{state()}</span><Show when={state() === 'closed'}><button class="primary tiny" onClick={() => connect()}>Reconnect</button></Show><span class="muted small">Tabs persist across reloads. Closing the browser detaches; × ends only that terminal.</span></div><Show when={note()}><p class="notice">{note()}</p></Show><div class="term-wrap"><div ref={host} style="height:100%" /></div></section>;
}
