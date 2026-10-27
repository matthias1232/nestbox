// Login.
//
// The password is read from the form at submit and never stored in a signal, so
// it does not sit in the component tree after the request.

import { createSignal } from 'solid-js';
import { useSession } from '../../state/session.jsx';

export default function LoginPage() {
  const session = useSession();
  const [email, setEmail] = createSignal('admin@nestbox.local');
  const [password, setPassword] = createSignal('');

  async function submit(event) {
    event.preventDefault();
    await session.signIn(email().trim(), password());
    setPassword('');
  }

  return (
    <div class="login">
      <form onSubmit={submit}>
        <h1>Nestbox</h1>
        <p class="muted">Control plane for governed agent sandboxes.</p>

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

        <p class="hint muted">
          The bootstrap password was printed once by the migration step.
        </p>
      </form>
    </div>
  );
}
