// Login.
//
// The password is read from the form at submit and never stored in a signal, so
// it does not sit in the component tree after the request.
//
// Identity providers (ADR 0046) are plain links: the browser leaves for the provider and comes
// back to the API, which sets the session cookie and redirects here. An invitation link
// (`/?invite=<token>`) is carried through the same trip, so the person lands in the right place.

import { For, Show, createSignal, onMount } from 'solid-js';
import { useSession } from '../../state/session.jsx';
import * as api from '../../api/tenancy.js';

export default function LoginPage() {
  const session = useSession();
  const params = new URLSearchParams(window.location.search);
  const invite = params.get('invite');
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [options, setOptions] = createSignal({ providers: [], registration_open: false });
  const [preview, setPreview] = createSignal(null);
  const [problem, setProblem] = createSignal(params.get('login_error'));

  onMount(async () => {
    try { setOptions(await api.signInOptions()); } catch { /* password sign-in still works */ }
    if (invite) {
      try { setPreview(await api.invitationPreview(invite)); }
      catch { setProblem('This invitation link is not valid any more. Ask for a new one.'); }
    }
  });

  async function submit(event) {
    event.preventDefault();
    await session.signIn(email().trim(), password());
    setPassword('');
  }

  const providerLink = (id) =>
    `/api/v1/auth/oidc/${id}/start${invite ? `?invitation=${encodeURIComponent(invite)}` : ''}`;

  return (
    <div class="login">
      <form onSubmit={submit}>
        <h1>Nestbox</h1>
        <p class="muted">Control plane for governed agent sandboxes.</p>

        <Show when={preview()}>
          <p class="notice">
            You are invited to <strong>{preview().data_center}</strong> of {preview().organization} as{' '}
            <strong>{preview().role}</strong>
            {preview().sandbox ? <> for the sandbox <strong>{preview().sandbox}</strong></> : null}.
            {preview().email ? <> Use {preview().email}.</> : null}
          </p>
        </Show>
        <Show when={problem()}><p class="error">{problem()}</p></Show>

        <Show when={options().providers.length > 0}>
          <div class="providers">
            <For each={options().providers}>
              {(provider) => (
                <a class="button" href={providerLink(provider.id)}>Continue with {provider.label}</a>
              )}
            </For>
          </div>
          <p class="muted small">
            {options().registration_open && !invite
              ? 'New here? Continue with a provider to create your workspace.'
              : invite ? '' : 'This installation is by invitation.'}
          </p>
          <hr />
        </Show>

        <Show when={!invite || options().providers.length === 0}>
          <label>
            <span>Email</span>
            <input value={email()} onInput={(e) => setEmail(e.currentTarget.value)}
                   autocomplete="username" required />
          </label>

          <label>
            <span>Password</span>
            <input type="password" value={password()}
                   onInput={(e) => setPassword(e.currentTarget.value)}
                   autocomplete="current-password" required />
          </label>

          {session.error() && <p class="error">{session.error()}</p>}

          <button type="submit" disabled={session.busy()}>
            {session.busy() ? 'Signing in…' : 'Sign in'}
          </button>
        </Show>
      </form>
    </div>
  );
}
