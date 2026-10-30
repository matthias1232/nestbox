// State for the master control panel: the organisation window, a sandbox
// drill-down, the request list, and the retention policy.
//
// No JSX here on purpose (`features/AGENTS.md`): the page renders, this file
// decides what a timeframe change, a drill-down, a save and a purge mean.

import { createSignal } from 'solid-js';
import * as api from '../../api/accounting.js';

// The closed set the server accepts. The page shows these five buttons; the
// server owns the bounds, so no other value is offered.
export const TIMEFRAMES = [
  { id: '5m', label: '5m', hint: 'the last five minutes' },
  { id: '1h', label: '1h', hint: 'the last hour' },
  { id: '24h', label: '24h', hint: 'the last twenty-four hours' },
  { id: '7d', label: '7d', hint: 'the last seven days' },
  { id: '30d', label: '30d', hint: 'the last thirty days' },
];

// One page of rows. The server clamps it, and the page says the list is a page.
const REQUEST_LIMIT = 100;

const MIN_RETENTION_DAYS = 1;
const MAX_RETENTION_DAYS = 3650;

const DEFAULT_RETENTION_DAYS = 30;

function settingsDraft(settings) {
  return {
    retention_days: Number.isFinite(settings?.retention_days)
      ? settings.retention_days
      : DEFAULT_RETENTION_DAYS,
    keep_daily: settings?.keep_daily !== false,
  };
}

function stamp(value) {
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? 0 : time;
}

// The server already answers newest first; sorting here keeps that promise if a
// future implementation does not, because the table says "newest first".
function newestFirst(items) {
  return [...items].sort((left, right) => stamp(right.created_at) - stamp(left.created_at));
}

function validDays(value) {
  const days = Number(value);
  return Number.isInteger(days) && days >= MIN_RETENTION_DAYS && days <= MAX_RETENTION_DAYS;
}

export function useMasterControl() {
  const [timeframe, setTimeframe] = createSignal('24h');
  const [sandbox, setSandbox] = createSignal(null);
  const [sandboxName, setSandboxName] = createSignal(null);

  const [overview, setOverview] = createSignal(null);
  const [requests, setRequests] = createSignal([]);
  const [loading, setLoading] = createSignal(true);
  const [requestsLoading, setRequestsLoading] = createSignal(true);
  const [error, setError] = createSignal(null);
  const [requestsError, setRequestsError] = createSignal(null);
  const [notice, setNotice] = createSignal(null);

  const [settings, setSettings] = createSignal(null);
  const [draft, setDraft] = createSignal(settingsDraft(null));
  const [settingsLoading, setSettingsLoading] = createSignal(true);
  const [settingsError, setSettingsError] = createSignal(null);
  const [saving, setSaving] = createSignal(false);
  const [purging, setPurging] = createSignal(false);
  const [pendingPurge, setPendingPurge] = createSignal(false);
  const [purgeDays, setPurgeDays] = createSignal(DEFAULT_RETENTION_DAYS);

  // Two counters, not one: the summary and the list are independent reads, and a
  // failed one must not cancel the other. Only the newest answer is applied, so
  // a slow timeframe cannot overwrite a faster one that came after it.
  let overviewGeneration = 0;
  let requestsGeneration = 0;

  async function loadOverview() {
    const mine = ++overviewGeneration;
    setLoading(true);
    setError(null);
    setOverview(null);
    try {
      const answer = await api.overview({
        timeframe: timeframe(),
        sandbox_id: sandbox() || undefined,
      });
      if (mine === overviewGeneration) setOverview(answer);
    } catch (failure) {
      if (mine === overviewGeneration) {
        setError(failure.message || 'The accounting overview could not be read.');
      }
    } finally {
      if (mine === overviewGeneration) setLoading(false);
    }
  }

  async function loadRequests() {
    const mine = ++requestsGeneration;
    setRequestsLoading(true);
    setRequestsError(null);
    setRequests([]);
    try {
      const answer = await api.requests({
        timeframe: timeframe(),
        sandbox_id: sandbox() || undefined,
        limit: REQUEST_LIMIT,
      });
      if (mine !== requestsGeneration) return;
      // The parallel backend answers `{ items }`; the first implementation
      // answers a bare array. Both are accepted, so a shape change is not a
      // screen that silently shows nothing.
      const items = Array.isArray(answer) ? answer : answer?.items || [];
      setRequests(newestFirst(Array.isArray(items) ? items : []));
    } catch (failure) {
      if (mine === requestsGeneration) {
        setRequestsError(failure.message || 'The request list could not be read.');
      }
    } finally {
      if (mine === requestsGeneration) setRequestsLoading(false);
    }
  }

  /// Read the summary and the list for the current timeframe and drill-down.
  async function load() {
    setNotice(null);
    setPendingPurge(false);
    await Promise.all([loadOverview(), loadRequests()]);
  }

  /// Change the window and keep the drill-down, because the two are one view.
  async function chooseTimeframe(id) {
    if (!TIMEFRAMES.some((frame) => frame.id === id)) return;
    setTimeframe(id);
    await load();
  }

  /// Narrow the whole screen to one sandbox. The name is kept here because a
  /// sandbox-scoped overview returns no per-sandbox list to read it from.
  async function drillDown(sandboxId, name) {
    if (!sandboxId) return;
    setSandbox(sandboxId);
    setSandboxName(name || null);
    await load();
  }

  async function clearDrillDown() {
    if (!sandbox()) return;
    setSandbox(null);
    setSandboxName(null);
    await load();
  }

  /// The retention setting, readable by anyone who may read telemetry.
  async function loadSettings() {
    setSettingsLoading(true);
    setSettingsError(null);
    try {
      const answer = await api.settings();
      const next = settingsDraft(answer);
      setSettings(answer);
      setDraft(next);
      setPurgeDays(next.retention_days);
    } catch (failure) {
      setSettingsError(failure.message || 'The retention settings could not be read.');
    } finally {
      setSettingsLoading(false);
    }
  }

  const setRetentionDays = (value) => setDraft({ ...draft(), retention_days: value });
  const setKeepDaily = (value) => setDraft({ ...draft(), keep_daily: Boolean(value) });

  /// Save both fields, because the server keeps one policy row per environment
  /// and a partial write would silently reset the half the form did not carry.
  async function saveSettings() {
    if (!validDays(draft().retention_days)) {
      setSettingsError(
        `Retention must be a whole number of days between ${MIN_RETENTION_DAYS} and ${MAX_RETENTION_DAYS}.`,
      );
      return;
    }
    setSaving(true);
    setSettingsError(null);
    setNotice(null);
    try {
      const answer = await api.saveSettings({
        retention_days: Number(draft().retention_days),
        keep_daily: Boolean(draft().keep_daily),
      });
      // The response is the saved row. If a server answers without it, the
      // values just sent are what the row now holds.
      const saved =
        answer && Number.isFinite(answer.retention_days)
          ? answer
          : {
              retention_days: Number(draft().retention_days),
              keep_daily: Boolean(draft().keep_daily),
            };
      const next = settingsDraft(saved);
      setSettings(saved);
      setDraft(next);
      setPurgeDays(next.retention_days);
      setNotice(
        `Raw requests are now kept for ${next.retention_days} days; daily rollups are ` +
          `${next.keep_daily ? 'kept' : 'not kept'}.`,
      );
    } catch (failure) {
      setSettingsError(failure.message || 'The retention setting could not be saved.');
    } finally {
      setSaving(false);
    }
  }

  const setPurgeDayCount = (value) => setPurgeDays(value);

  /// A destructive action asks once and says what it destroys.
  const askPurge = () => setPendingPurge(true);
  const cancelPurge = () => setPendingPurge(false);

  async function confirmPurge() {
    if (!validDays(purgeDays())) {
      setSettingsError(
        `The purge day count must be a whole number between ${MIN_RETENTION_DAYS} and ${MAX_RETENTION_DAYS}.`,
      );
      return;
    }
    const days = Number(purgeDays());
    setPurging(true);
    setPendingPurge(false);
    setSettingsError(null);
    setNotice(null);
    try {
      const answer = await api.purge(days);
      const removed =
        typeof answer?.removed_requests === 'number'
          ? `${answer.removed_requests.toLocaleString()} raw request(s) removed.`
          : 'The purge finished.';
      const rolled =
        typeof answer?.rolled_up_rows === 'number'
          ? ` ${answer.rolled_up_rows.toLocaleString()} daily rollup row(s) written.`
          : '';
      await load();
      setNotice(`${removed}${rolled}`);
    } catch (failure) {
      setError(failure.message || 'The purge failed.');
    } finally {
      setPurging(false);
    }
  }

  return {
    TIMEFRAMES,
    REQUEST_LIMIT,
    timeframe,
    sandbox,
    sandboxName,
    overview,
    requests,
    loading,
    requestsLoading,
    error,
    requestsError,
    notice,
    settings,
    draft,
    settingsLoading,
    settingsError,
    saving,
    purging,
    pendingPurge,
    purgeDays,
    load,
    chooseTimeframe,
    drillDown,
    clearDrillDown,
    loadSettings,
    setRetentionDays,
    setKeepDaily,
    saveSettings,
    setPurgeDayCount,
    askPurge,
    cancelPurge,
    confirmPurge,
  };
}
