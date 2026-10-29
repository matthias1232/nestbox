// Who may open this sandbox's SSH door, besides the key the control plane made.
//
// Two ways, both administered here: public keys an operator adds (their own
// laptop key), and a temporary user name and password for a client that has no
// key. A password is shown once, at the moment it is made, with the lines to use it.

import { For, Show, createResource, createSignal } from 'solid-js';
import * as api from '../../api/sandboxes.js';

const LIFETIMES = [
  { minutes: 15, label: '15 minutes' },
  { minutes: 60, label: '1 hour' },
  { minutes: 480, label: '8 hours' },
  { minutes: 1440, label: '24 hours' },
];

function when(value) {
  return value ? new Date(value).toLocaleString() : '';
}

export default function SshAccessSection(props) {
  const [access, { refetch }] = createResource(() => props.sandboxId, api.sshAccess);
  const [label, setLabel] = createSignal('');
  const [keyText, setKeyText] = createSignal('');
  const [minutes, setMinutes] = createSignal(60);
  const [issued, setIssued] = createSignal(null);
  const [failure, setFailure] = createSignal(null);

  async function run(action) {
    setFailure(null);
    try {
      await action();
      await refetch();
      props.onChanged?.();
    } catch (error) {
      setFailure(error.message);
    }
  }

  const sshLine = () => {
    const port = props.info?.ports.find((item) => item.purpose === 'ssh');
    if (!issued() || !port || !props.info?.host) return null;
    return `ssh -p ${port.public_port} ${issued().username}@${props.info.host}`;
  };

  return (
    <section class="connect-block ssh-access">
      <h3>SSH access</h3>
      <p class="muted small">
        Besides the generated key you can add your own public key, or make a temporary user name and
        password for a client without a key. Changes reach the SSH door within seconds.
      </p>
      <Show when={failure()}><p class="error">{failure()}</p></Show>

      <h4>Your keys</h4>
      <Show when={access()?.generated_key_fingerprint}>
        <p class="small">Generated key <span class="mono">{access().generated_key_fingerprint}</span></p>
      </Show>
      <Show when={(access()?.keys || []).length > 0} fallback={<p class="muted small">No key of your own is added.</p>}>
        <table>
          <thead><tr><th>Label</th><th>Fingerprint</th><th>Added</th><th /></tr></thead>
          <tbody>
            <For each={access().keys}>
              {(key) => (
                <tr>
                  <td>{key.label}</td>
                  <td class="mono small">{key.fingerprint}</td>
                  <td class="small">{when(key.created_at)}</td>
                  <td><button class="tiny" onClick={() => run(() => api.removeUserKey(props.sandboxId, key.id))}>Remove</button></td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
      <form
        class="controls"
        onSubmit={(event) => {
          event.preventDefault();
          run(async () => {
            await api.addUserKey(props.sandboxId, label(), keyText());
            setLabel('');
            setKeyText('');
          });
        }}
      >
        <input placeholder="label, e.g. work laptop" value={label()} onInput={(e) => setLabel(e.currentTarget.value)} />
        <input class="mono" style="min-width:320px" placeholder="ssh-ed25519 AAAA… (the .pub file)" value={keyText()} onInput={(e) => setKeyText(e.currentTarget.value)} />
        <button class="primary tiny" type="submit" disabled={!label().trim() || !keyText().trim()}>Add key</button>
      </form>

      <h4>Temporary login (user name and password)</h4>
      <div class="controls">
        <select value={minutes()} onChange={(e) => setMinutes(Number(e.currentTarget.value))}>
          <For each={LIFETIMES}>{(item) => <option value={item.minutes}>valid {item.label}</option>}</For>
        </select>
        <button class="primary tiny" onClick={() => run(async () => setIssued(await api.issueCredential(props.sandboxId, minutes())))}>
          Create temporary login
        </button>
      </div>
      <Show when={issued()}>
        <div class="notice">
          <p><b>Shown once.</b> Copy it now; it cannot be read again.</p>
          <p>User <span class="mono big">{issued().username}</span> · Password <span class="mono big">{issued().password}</span></p>
          <p class="small">Valid until {when(issued().expires_at)}.</p>
          <Show when={sshLine()}><pre class="mono">{sshLine()}</pre></Show>
          <button class="tiny" onClick={() => setIssued(null)}>Hide</button>
        </div>
      </Show>
      <Show when={(access()?.credentials || []).length > 0}>
        <table>
          <thead><tr><th>User</th><th>State</th><th>Valid until</th><th /></tr></thead>
          <tbody>
            <For each={access().credentials}>
              {(credential) => (
                <tr>
                  <td class="mono small">{credential.username}</td>
                  <td class="small">{credential.state}</td>
                  <td class="small">{when(credential.expires_at)}</td>
                  <td>
                    <Show when={credential.state === 'active'}>
                      <button class="tiny" onClick={() => run(() => api.revokeCredential(props.sandboxId, credential.id))}>Revoke</button>
                    </Show>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </section>
  );
}
