// Users of this environment (admins only).
//
// A user manages their own sandboxes from start to end; an admin sees and manages all of
// them and sets what everyone shares (providers and keys, policies, gateways). Roles:
//   admin    everything, including users, providers, keys and global policy;
//   operator their own sandboxes: create, run, policies on them, terminal, backups;
//   viewer   read-only, on their own sandboxes and the shared ones.

import { For, Show, createSignal, onMount } from 'solid-js';
import * as api from '../../api/users.js';

const ROLES = [
  ['admin', 'Admin — everything, including users, providers, keys and global policy'],
  ['operator', 'Operator — manages their own sandboxes from start to end'],
  ['viewer', 'Viewer — read-only on their own and shared sandboxes'],
];

const blank = { email: '', display_name: '', password: '', role: 'operator' };

export default function UsersPage() {
  const [items, setItems] = createSignal([]);
  const [form, setForm] = createSignal({ ...blank });
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [busy, setBusy] = createSignal(false);
  const [passwordFor, setPasswordFor] = createSignal(null);
  const [newPassword, setNewPassword] = createSignal('');

  async function load() {
    try {
      setItems((await api.list()).items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }
  onMount(load);

  async function guard(action, done) {
    setError(null);
    setNotice(null);
    setBusy(true);
    try {
      await action();
      if (done) setNotice(done);
      await load();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  const submit = (event) => {
    event.preventDefault();
    guard(async () => {
      await api.create(form());
      setForm({ ...blank });
    }, 'User created. Give them the password through a channel of your own; it is not shown again.');
  };

  const changeRole = (user, role) =>
    guard(() => api.update(user.id, { role }), `${user.display_name} is now ${role}. Their sessions ended; they sign in again.`);
  const toggle = (user) =>
    guard(
      () => api.update(user.id, { status: user.status === 'active' ? 'disabled' : 'active' }),
      user.status === 'active' ? `${user.display_name} is disabled and signed out.` : `${user.display_name} is active again.`,
    );
  const remove = (user) => {
    if (!window.confirm(`Remove ${user.display_name} from this environment? Their sandboxes stay and become shared; an admin can reassign or delete them.`)) return;
    guard(() => api.remove(user.id), `${user.display_name} was removed.`);
  };
  const savePassword = (event) => {
    event.preventDefault();
    const user = passwordFor();
    guard(async () => {
      await api.setPassword(user.id, newPassword());
      setPasswordFor(null);
      setNewPassword('');
    }, `New password set for ${user.display_name}; their sessions ended.`);
  };

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Users</h1>
          <p class="muted">
            {items().length} user{items().length === 1 ? '' : 's'} in this environment. Each user sees
            and manages their own sandboxes; admins see all of them.
          </p>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>
      <Show when={notice()}><p class="notice">{notice()}</p></Show>

      <div class="table-scroll">
        <table>
          <thead>
            <tr><th>Name</th><th>Email</th><th>Role</th><th>Status</th><th>Sandboxes</th><th /></tr>
          </thead>
          <tbody>
            <For each={items()}>
              {(user) => (
                <tr>
                  <td>{user.display_name}{user.is_you ? ' (you)' : ''}</td>
                  <td class="mono small">{user.email}</td>
                  <td>
                    <select value={user.role} disabled={busy() || user.is_you}
                            title={user.is_you ? 'You cannot change your own role' : 'Change the role'}
                            onChange={(e) => changeRole(user, e.currentTarget.value)}>
                      <For each={ROLES}>{([value]) => <option value={value}>{value}</option>}</For>
                    </select>
                  </td>
                  <td>{user.status}</td>
                  <td>{user.sandboxes}</td>
                  <td class="actions">
                    <button class="tiny" onClick={() => setPasswordFor(user)}>Set password…</button>
                    <button class="tiny" disabled={busy() || user.is_you} onClick={() => toggle(user)}>
                      {user.status === 'active' ? 'Disable' : 'Enable'}
                    </button>
                    <button class="tiny danger" disabled={busy() || user.is_you} onClick={() => remove(user)}>Remove</button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>

      <Show when={passwordFor()}>
        <form class="editor" onSubmit={savePassword}>
          <h3>New password for {passwordFor().display_name}</h3>
          <label class="field">
            <span>Password</span>
            <input type="password" autocomplete="new-password" minlength="12" required
                   value={newPassword()} onInput={(e) => setNewPassword(e.currentTarget.value)} />
            <small class="muted">At least 12 characters. Their current sessions end, so they sign in again.</small>
          </label>
          <div class="actions">
            <button type="submit" disabled={busy()}>Set password</button>
            <button type="button" class="secondary" onClick={() => setPasswordFor(null)}>Cancel</button>
          </div>
        </form>
      </Show>

      <form class="editor" onSubmit={submit}>
        <h3>Add a user</h3>
        <div class="form-grid">
          <label class="field">
            <span>Email</span>
            <input type="email" required value={form().email} placeholder="name@example.com"
                   onInput={(e) => setForm({ ...form(), email: e.currentTarget.value })} />
            <small class="muted">The user signs in with it.</small>
          </label>
          <label class="field">
            <span>Name</span>
            <input required maxlength="80" value={form().display_name} placeholder="Ada Lovelace"
                   onInput={(e) => setForm({ ...form(), display_name: e.currentTarget.value })} />
            <small class="muted">Shown in lists and next to their sandboxes.</small>
          </label>
          <label class="field">
            <span>Role</span>
            <select value={form().role} onChange={(e) => setForm({ ...form(), role: e.currentTarget.value })}>
              <For each={ROLES}>{([value, text]) => <option value={value}>{text}</option>}</For>
            </select>
            <small class="muted">Most people are operators: they manage their own sandboxes and nothing shared.</small>
          </label>
          <label class="field">
            <span>Initial password</span>
            <input type="password" autocomplete="new-password" required minlength="12" value={form().password}
                   onInput={(e) => setForm({ ...form(), password: e.currentTarget.value })} />
            <small class="muted">At least 12 characters. Hand it over once; they can be given a new one any time.</small>
          </label>
        </div>
        <div class="actions"><button type="submit" disabled={busy()}>Create user</button></div>
      </form>
    </section>
  );
}
