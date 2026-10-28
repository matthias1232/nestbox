// How to reach one sandbox from your own machine.
//
// One read of `GET /sandboxes/{id}/connect`: the node's address and hostname
// (what a client dials, never the container address), every published port, the
// SSH key state, and the commands already filled in. Used by the Access tab and
// by the Connect drawer in the list, so the two can never disagree.

import { For, Show, createResource, createSignal } from 'solid-js';
import * as sandboxes from '../../api/sandboxes.js';
import * as nodes from '../../api/nodes.js';
import '../../styles/connect.css';

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

function CopyButton(props) {
  const [done, setDone] = createSignal(false);
  return (
    <button
      class="tiny"
      title={`Copy ${props.what || 'to the clipboard'}`}
      onClick={async () => {
        if (await copy(props.text)) {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        }
      }}
    >
      {done() ? 'Copied' : 'Copy'}
    </button>
  );
}

const PURPOSE = { ssh: 'SSH', agent: 'Agent', custom: 'Custom' };

export default function ConnectPanel(props) {
  const [info, { refetch }] = createResource(() => props.sandboxId, sandboxes.connect);
  const [message, setMessage] = createSignal(null);
  const [failure, setFailure] = createSignal(null);

  async function run(action, done) {
    setFailure(null);
    setMessage(null);
    try {
      await action();
      if (done) setMessage(done);
      await refetch();
    } catch (error) {
      setFailure(error.message);
    }
  }

  // The private key is shown once; saving it as a file is the one chance, so the
  // file is offered straight away instead of being displayed on screen.
  function downloadKey() {
    return run(async () => {
      const key = await sandboxes.downloadSshKey(props.sandboxId);
      const blob = new Blob([key.private_key_pem], { type: 'application/x-pem-file' });
      const link = document.createElement('a');
      link.href = URL.createObjectURL(blob);
      link.download = key.filename || `nestbox-${info()?.sandbox_name}`;
      link.click();
      URL.revokeObjectURL(link.href);
    }, 'The key was downloaded. It cannot be downloaded again; rotate it if you lose it.');
  }

  function rotateKey() {
    if (!window.confirm('Rotate the SSH key? The old key stops working and a new one can be downloaded once.')) return;
    return run(() => sandboxes.rotateSshKey(props.sandboxId), 'A new key is ready to download.');
  }

  const [address, setAddress] = createSignal('');
  const [hostname, setHostname] = createSignal('');
  function saveEndpoint(event) {
    event.preventDefault();
    return run(
      () => nodes.setEndpoint(info().node_id, { public_address: address(), public_hostname: hostname() }),
      'Saved. Ports published before this keep the binding they were made with.',
    );
  }

  const identity = () => `~/.ssh/nestbox-${info()?.sandbox_name}`;

  return (
    <div class="connect">
      <Show when={info.error}>
        <p class="error">{info.error.message}</p>
      </Show>
      <Show when={failure()}><p class="error">{failure()}</p></Show>
      <Show when={message()}><p class="notice">{message()}</p></Show>

      <Show when={info()} fallback={<p class="muted">Reading how to reach this sandbox…</p>}>
        {(data) => (
          <>
            <section class="connect-host">
              <span class="label">Connect to</span>
              <Show
                when={data().host}
                fallback={<span class="mono muted">no address set on {data().node_name}</span>}
              >
                <span class="mono big">{data().host}</span>
                <CopyButton text={data().host} what="the host" />
              </Show>
              <small class="muted">
                Node {data().node_name}
                <Show when={data().hostname}> · hostname <span class="mono">{data().hostname}</span></Show>
                <Show when={data().address}> · address <span class="mono">{data().address}</span></Show>
              </small>
            </section>

            <Show when={data().hint}>
              <div class="notice warn">
                <p>{data().hint}</p>
                <Show when={!data().host || data().hint.includes('127.0.0.1')}>
                  <form class="connect-endpoint" onSubmit={saveEndpoint}>
                    <label class="field">
                      <span>Address clients dial</span>
                      <input
                        placeholder="192.168.1.20"
                        value={address()}
                        onInput={(e) => setAddress(e.currentTarget.value)}
                      />
                    </label>
                    <label class="field">
                      <span>Hostname (optional)</span>
                      <input
                        placeholder="box.example.net"
                        value={hostname()}
                        onInput={(e) => setHostname(e.currentTarget.value)}
                      />
                    </label>
                    <button class="primary" type="submit">Save for {data().node_name}</button>
                  </form>
                </Show>
              </div>
            </Show>

            <Show when={data().ports.length > 0}>
              <h3>Published ports</h3>
              <div class="table-scroll">
                <table>
                  <thead>
                    <tr><th>For</th><th>In the sandbox</th><th>Dial</th><th>State</th><th /></tr>
                  </thead>
                  <tbody>
                    <For each={data().ports}>
                      {(port) => (
                        <tr>
                          <td>
                            <b>{port.label || PURPOSE[port.purpose] || port.purpose}</b>
                            <Show when={port.label}><small class="muted"> · {PURPOSE[port.purpose]}</small></Show>
                          </td>
                          <td class="mono small">{port.target_port}</td>
                          <td class="mono small">
                            {port.dial || <span class="muted">:{port.public_port}</span>}
                          </td>
                          <td class="small">{port.state}</td>
                          <td class="actions">
                            <Show when={port.dial}><CopyButton text={port.dial} what="host:port" /></Show>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            <For each={data().ports.filter((port) => port.command)}>
              {(port) => (
                <section class="connect-block">
                  <h3>SSH</h3>
                  <pre class="mono">{port.command}</pre>
                  <CopyButton text={port.command} what="the ssh command" />
                </section>
              )}
            </For>

            <Show when={data().ssh_config}>
              <section class="connect-block">
                <h3>~/.ssh/config</h3>
                <p class="muted small">
                  With this, <span class="mono">ssh {data().sandbox_name}</span> works, and so does
                  VS Code Remote-SSH or any tool that reads your SSH config.
                </p>
                <pre class="mono">{data().ssh_config}</pre>
                <CopyButton text={data().ssh_config} what="the SSH config block" />
              </section>
            </Show>

            <section class="connect-block">
              <h3>SSH key</h3>
              <Show
                when={data().key}
                fallback={<p class="muted">No key exists for this sandbox yet.</p>}
              >
                {(key) => (
                  <>
                    <p class="small">
                      Fingerprint <span class="mono">{key().fingerprint}</span>
                    </p>
                    <Show
                      when={key().downloadable}
                      fallback={
                        <p class="muted small">
                          Already downloaded. Save it as <span class="mono">{identity()}</span>. Lost
                          it? Rotate to get a new one.
                        </p>
                      }
                    >
                      <p class="small">
                        Download it once, save it as <span class="mono">{identity()}</span>, then run{' '}
                        <span class="mono">chmod 600 {identity()}</span>.
                      </p>
                      <button class="primary" onClick={downloadKey}>Download private key</button>
                    </Show>
                    <button class="secondary" onClick={rotateKey}>Rotate key</button>
                  </>
                )}
              </Show>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}
