// State for the media screen: the environment default, the provider list, one
// sandbox's override and its recent renders.
//
// No JSX here on purpose (`features/AGENTS.md`): the page renders, this file
// decides what "saving" means and what an error says.

import { createSignal } from 'solid-js';
import * as api from '../../api/media.js';

const KINDS = [
  { id: 'image', label: 'Images', tool: 'render_image' },
  { id: 'video', label: 'Video', tool: 'render_video' },
  { id: 'music', label: 'Music', tool: 'render_music' },
];

export function useMedia() {
  const [view, setView] = createSignal(null);
  const [sandboxView, setSandboxView] = createSignal(null);
  const [renders, setRenders] = createSignal([]);
  const [users, setUsers] = createSignal([]);
  const [error, setError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  async function load() {
    setError(null);
    try {
      setView(await api.settings());
    } catch (failure) {
      setError(failure.message || 'The media settings could not be read.');
    }
  }

  /// The admin list of releases. A failure here is not a page error: the
  /// default above still works, and a viewer is not supposed to read it.
  async function loadUsers() {
    try {
      setUsers(await api.users());
    } catch {
      setUsers([]);
    }
  }

  async function saveUserGrant(userId, draft) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.setUserGrant(userId, draft);
      await loadUsers();
      setNotice('Release saved.');
    } catch (failure) {
      setError(failure.message || 'The release could not be saved.');
    } finally {
      setBusy(false);
    }
  }

  async function loadSandbox(sandboxId) {
    setSandboxView(null);
    setRenders([]);
    if (!sandboxId) return;
    try {
      setSandboxView(await api.sandbox(sandboxId));
      setRenders(await api.renders(sandboxId));
    } catch (failure) {
      setError(failure.message || 'The sandbox media settings could not be read.');
    }
  }

  // `draft` is the whole settings object, not a field: the server replaces the
  // row, so a partial write would silently clear a kind the form did not carry.
  async function save(draft) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      setView(await api.saveSettings(draft));
      setNotice('Environment default saved. Sandboxes that inherit use it on their next render.');
    } catch (failure) {
      setError(failure.message || 'The media settings could not be saved.');
    } finally {
      setBusy(false);
    }
  }

  async function saveSandbox(sandboxId, draft) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const answer = await api.saveSandbox(sandboxId, draft);
      setSandboxView(answer);
      setNotice('Sandbox choice saved.');
    } catch (failure) {
      setError(failure.message || 'The sandbox choice could not be saved.');
    } finally {
      setBusy(false);
    }
  }

  return {
    KINDS,
    view,
    users,
    sandboxView,
    renders,
    error,
    notice,
    busy,
    load,
    loadUsers,
    saveUserGrant,
    loadSandbox,
    save,
    saveSandbox,
  };
}
