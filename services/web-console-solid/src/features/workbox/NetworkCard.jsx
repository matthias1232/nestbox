// The proxy mode of one sandbox and the temporary windows that pause the rules.
//
// Every switch here is one request to the control API and then a fresh control
// summary; the proxy answers the next CONNECT with the new mode, so nothing in
// this card needs a restart. The one exception is a sandbox that was created
// before the CA was mounted everywhere: it says so once and offers the recreate
// instead of pretending TLS will be readable.

import { For, Show, createSignal } from 'solid-js';

import Countdown from '../../components/Countdown.jsx';
import ConfirmDialog from '../../components/ConfirmDialog.jsx';
import * as deck from '../../api/deck.js';
import * as policies from '../../api/policies.js';
import {
  CHANNELS,
  DURATIONS,
  MODES,
  channelLabel,
  channelsFor,
  modeLabel,
  openWindows,
} from './network.js';

// The proxy mode card of one sandbox: four live choices, the open windows and
// the one hint that needs a recreate.
export default function NetworkCard(props) {
  const sandboxId = () => props.sandboxId;
  const control = () => props.control || {};
  const egress = () => control().egress || {};
  const mode = () => egress().mode || 'forward_proxy';
  const windows = () => openWindows(control());
  const help = (value) => (MODES.find((item) => item.value === value) || {}).help
    || (value === 'direct' ? 'No proxy at all: an administrator took it out of the path under Policies.' : '');

  const [failure, setFailure] = createSignal(null);
  const [message, setMessage] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  // The dialog that turns the proxy off, with the duration it should last.
  const [offOpen, setOffOpen] = createSignal(false);
  const [offMinutes, setOffMinutes] = createSignal(15);

  // The dialog that opens an evaluation window without touching the mode.
  const [pauseOpen, setPauseOpen] = createSignal(false);
  const [pauseMinutes, setPauseMinutes] = createSignal(15);
  const [pauseChannels, setPauseChannels] = createSignal(CHANNELS.map((item) => item.value));
  const [pauseMode, setPauseMode] = createSignal('audit');
  const [pauseReason, setPauseReason] = createSignal('Evaluation from the workbox');

  const [recreateOpen, setRecreateOpen] = createSignal(false);

  // One place for the three states a call has: it failed, it worked, it runs.
  async function run(action, done) {
    setFailure(null);
    setMessage(null);
    setBusy(true);
    try {
      await action();
      if (done) setMessage(done);
      props.onChanged?.();
    } catch (error) {
      setFailure(error.message + (error.code ? ` (${error.code})` : ''));
    } finally {
      setBusy(false);
    }
  }

  function choose(value) {
    if (value === mode()) return;
    if (value === 'off_allow_all') {
      setOffMinutes(15);
      setOffOpen(true);
      return;
    }
    run(
      () => policies.setEgressMode(sandboxId(), value, null),
      `Mode changed to "${modeLabel(value)}" — it applies to new connections within seconds.`,
    );
  }

  function allowEverything() {
    const minutes = offMinutes();
    setOffOpen(false);
    run(
      () => policies.setEgressMode(sandboxId(), 'off_allow_all', minutes),
      'The proxy is off: everything is allowed and only recorded.',
    );
  }

  function toggleChannel(value) {
    setPauseChannels((current) => (current.includes(value)
      ? current.filter((item) => item !== value)
      : [...current, value]));
  }

  function pause() {
    const channels = channelsFor(pauseChannels());
    if (!channels.length) {
      setFailure('Tick at least one thing to allow.');
      return;
    }
    const minutes = pauseMinutes();
    const reason = pauseReason().trim() || 'Evaluation from the workbox';
    // An unlimited window still carries a number the server can check; the flag
    // is what makes it stay open.
    const limit = minutes ?? 15;
    const unlimited = minutes === null;
    setPauseOpen(false);
    run(async () => {
      for (const channel of channels) {
        await policies.openWindow(sandboxId(), limit, reason, pauseMode(), channel, unlimited);
      }
    }, pauseMode() === 'audit'
      ? 'The rules are paused and every decision is recorded.'
      : 'The rules are paused and nothing is recorded.');
  }

  return (
    <section class="wb-card">
      <h2>Network · proxy</h2>

      <Show when={failure()}>
        <p class="error small">{failure()}</p>
      </Show>
      <Show when={message()}>
        <p class="notice small">{message()}</p>
      </Show>

      <div
        class="nb-segmented"
        role="radiogroup"
        aria-label="How deeply the proxy observes this sandbox"
        style="display: flex; flex-direction: column; align-items: stretch; gap: 4px;"
      >
        <For each={MODES}>
          {(item) => (
            <button
              type="button"
              role="radio"
              aria-checked={mode() === item.value}
              class={'nb-seg' + (mode() === item.value ? ' active' : '') + (item.tone ? ` tone-${item.tone}` : '')}
              disabled={busy()}
              onClick={() => choose(item.value)}
            >
              {item.label}
            </button>
          )}
        </For>
      </div>
      <p class="muted small">{help(mode())}</p>
      <p class="muted small">Applies live; nothing restarts.</p>

      <Show when={egress().revert_at}>
        <p class="small">
          <Countdown prefix="Back to the previous mode in " until={egress().revert_at} onExpire={() => props.onChanged?.()} />
        </p>
      </Show>

      <Show when={egress().needs_recreate}>
        <div class="warn-note">
          <p>This sandbox was made before live switching; recreate it once so TLS can be opened.</p>
          <button class="secondary tiny" disabled={busy()} onClick={() => setRecreateOpen(true)}>Recreate once</button>
        </div>
      </Show>

      <h3>Pause the rules</h3>
      <p class="muted small">
        Let everything through for a while without switching the proxy. Whatever a rule would have
        blocked is recorded, so the traffic becomes the evidence for the rules you write afterwards.
      </p>
      <button class="secondary tiny" disabled={busy()} onClick={() => setPauseOpen(true)}>Allow everything…</button>

      <Show when={windows().length}>
        <ul class="plain">
          <For each={windows()}>
            {(entry) => (
              <li>
                <span class="badge info">
                  <i />
                  {entry.mode === 'bypass' ? 'allowed quietly' : 'allowed and recorded'} · {channelLabel(entry.channel)}
                </span>{' '}
                <Show when={entry.expires_at} fallback={<span class="muted small">until you turn it off</span>}>
                  <Countdown prefix="ends in " until={entry.expires_at} onExpire={() => props.onChanged?.()} />
                </Show>
                {' '}
                <button
                  class="tiny ghost"
                  disabled={busy()}
                  onClick={() => run(() => policies.closeWindow(entry.id), 'The window is closed.')}
                >
                  End
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <ConfirmDialog
        open={offOpen()}
        title="Turn the proxy off and allow everything?"
        onClose={() => setOffOpen(false)}
        actions={[
          { label: 'Allow everything', tone: 'danger', onClick: allowEverything },
          { label: 'Cancel', tone: 'secondary', onClick: () => setOffOpen(false) },
        ]}
      >
        <p>{help('off_allow_all')}</p>
        <p class="muted small">
          The rules are not enforced while the proxy is off. What they would have blocked is still
          recorded, and "Suggest rules" on the banner turns it into drafts.
        </p>
        <p class="small">For how long?</p>
        <div class="controls">
          <For each={DURATIONS}>
            {(entry) => (
              <button
                type="button"
                class={'tiny ' + (offMinutes() === entry.minutes ? 'primary' : 'secondary')}
                onClick={() => setOffMinutes(entry.minutes)}
              >
                {entry.label}
              </button>
            )}
          </For>
        </div>
        <p class="muted small">
          {offMinutes() === null
            ? 'It stays off until you switch it back.'
            : 'It switches back to the previous mode by itself.'}
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={pauseOpen()}
        title="Allow everything for a while?"
        onClose={() => setPauseOpen(false)}
        actions={[
          { label: 'Allow everything', tone: 'primary', onClick: pause },
          { label: 'Cancel', tone: 'secondary', onClick: () => setPauseOpen(false) },
        ]}
      >
        <p class="small">For how long?</p>
        <div class="controls">
          <For each={DURATIONS}>
            {(entry) => (
              <button
                type="button"
                class={'tiny ' + (pauseMinutes() === entry.minutes ? 'primary' : 'secondary')}
                onClick={() => setPauseMinutes(entry.minutes)}
              >
                {entry.label}
              </button>
            )}
          </For>
        </div>
        <p class="small">What may pass?</p>
        <div class="controls">
          <For each={CHANNELS}>
            {(item) => (
              <label class="small">
                <input
                  type="checkbox"
                  checked={pauseChannels().includes(item.value)}
                  onChange={() => toggleChannel(item.value)}
                />{' '}
                {item.label}
              </label>
            )}
          </For>
        </div>
        <p class="small">How is it recorded?</p>
        <div class="controls">
          <label class="small">
            <input
              type="radio"
              name="nb-pause-mode"
              checked={pauseMode() === 'audit'}
              onChange={() => setPauseMode('audit')}
            />{' '}
            Allow and record
          </label>
          <label class="small">
            <input
              type="radio"
              name="nb-pause-mode"
              checked={pauseMode() === 'bypass'}
              onChange={() => setPauseMode('bypass')}
            />{' '}
            Allow quietly
          </label>
        </div>
        <label class="field">
          <span>Reason</span>
          <input value={pauseReason()} onInput={(e) => setPauseReason(e.currentTarget.value)} />
        </label>
      </ConfirmDialog>

      <ConfirmDialog
        open={recreateOpen()}
        title="Recreate this sandbox once?"
        onClose={() => setRecreateOpen(false)}
        actions={[
          {
            label: 'Recreate',
            tone: 'danger',
            onClick: () => {
              setRecreateOpen(false);
              run(() => deck.recreate(sandboxId()), 'The sandbox was recreated; TLS can be opened now.');
            },
          },
          { label: 'Cancel', tone: 'secondary', onClick: () => setRecreateOpen(false) },
        ]}
      >
        <p>
          The container is made again with the CA mounted. Persistent folders and logins stay;
          anything else inside the container is lost, and open terminals end.
        </p>
      </ConfirmDialog>
    </section>
  );
}
