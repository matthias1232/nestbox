// The sandbox's graphical desktop, in the browser.
//
// noVNC speaks the VNC protocol over the WebSocket the control API opens to the
// sandbox (through the runner). The desktop is the sandbox's own: XFCE and the
// same Chrome an agent drives through the browser MCP, so a person can watch,
// take over, and log in where an agent is asked to.
//
// Only a sandbox built with the desktop pack has one. For the others the panel
// says how to get one instead of showing a black rectangle.
import { Show, createSignal, onCleanup, onMount } from 'solid-js';
import RFB from '@novnc/novnc/lib/rfb';
import * as deck from '../../api/deck.js';

export default function DesktopTab(props) {
  let screen;
  let rfb;
  const [state, setState] = createSignal('idle');
  const [note, setNote] = createSignal(null);
  const [viewOnly, setViewOnly] = createSignal(false);
  const [browser, setBrowser] = createSignal(null);
  const running = () => props.sandbox.status === 'running';

  async function loadBrowser() {
    try {
      const attached = await deck.mcpAttached(props.sandbox.id);
      setBrowser((attached.items || attached || []).some?.((item) => (item.server_key || item.key) === 'chrome-browser'));
    } catch {
      setBrowser(null);
    }
  }

  function connect() {
    disconnect();
    if (!running()) return;
    setNote(null);
    setState('connecting');
    const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
    rfb = new RFB(screen, `${scheme}://${location.host}/api/v1/sandboxes/${props.sandbox.id}/desktop`, {
      wsProtocols: ['binary'],
    });
    rfb.scaleViewport = true;
    rfb.resizeSession = false;
    rfb.viewOnly = viewOnly();
    rfb.addEventListener('connect', () => setState('open'));
    rfb.addEventListener('disconnect', (event) => {
      setState('closed');
      if (!event.detail.clean) {
        setNote('The desktop did not answer. This sandbox probably has no desktop: build it from the "Desktop (XFCE, Chrome, VNC)" pack in the image builder.');
      }
    });
    rfb.addEventListener('clipboard', (event) => {
      // Text copied inside the desktop goes to the person's clipboard too.
      const area = document.createElement('textarea');
      area.value = event.detail.text;
      area.style.cssText = 'position:fixed;opacity:0';
      document.body.appendChild(area);
      area.select();
      try { document.execCommand('copy'); } catch { /* the desktop shows its own selection */ }
      document.body.removeChild(area);
    });
  }

  function disconnect() {
    try { rfb?.disconnect(); } catch { /* already closed */ }
    rfb = null;
  }

  async function giveBrowser() {
    try {
      await deck.mcpAttach(props.sandbox.id, 'chrome-browser');
      await deck.reconfigureAgent(props.sandbox.id);
      setNote('The browser is attached. Agents started from now on know it; restart a running agent so it reads its config again.');
      loadBrowser();
    } catch (failure) {
      setNote(failure.message);
    }
  }

  onMount(() => { connect(); loadBrowser(); });
  onCleanup(disconnect);

  return (
    <section class="tab-panel">
      <div class="controls">
        <span class={`badge ${state() === 'open' ? 'ok' : state() === 'connecting' ? 'warn' : 'muted'}`}><i />{state()}</span>
        <button class="tiny" onClick={connect} disabled={!running()}>{state() === 'open' ? 'Reconnect' : 'Connect'}</button>
        <label class="small">
          <input type="checkbox" checked={viewOnly()} onChange={(e) => { setViewOnly(e.currentTarget.checked); if (rfb) rfb.viewOnly = e.currentTarget.checked; }} />
          {' '}watch only
        </label>
        <button class="tiny secondary" onClick={() => screen?.requestFullscreen?.()}>Full screen</button>
        <Show when={browser() === false}>
          <button class="tiny" onClick={giveBrowser} title="Attach the Chrome DevTools MCP server so agents can open pages and take screenshots in this Chrome">Give agents this browser (MCP)</button>
        </Show>
        <Show when={browser() === true}>
          <span class="muted small">Agents drive this Chrome through the “chrome-browser” MCP server.</span>
        </Show>
      </div>
      <Show when={!running()}><p class="muted">The sandbox is not running.</p></Show>
      <Show when={note()}><p class="notice">{note()}</p></Show>
      <div ref={screen} style="height:70vh;background:#111;border-radius:6px;overflow:hidden" />
    </section>
  );
}
