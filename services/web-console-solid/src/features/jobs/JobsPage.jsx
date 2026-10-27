// Jobs and schedules.

import { For, Show, createSignal, onMount } from 'solid-js';
import StatusBadge from '../../components/StatusBadge.jsx';
import Tabs from '../../components/Tabs.jsx';
import * as api from '../../api/jobs.js';

const TABS = [
  { id: 'jobs', label: 'Jobs' },
  { id: 'schedules', label: 'Schedules' },
];

export default function JobsPage() {
  const [tab, setTab] = createSignal('jobs');
  const [jobs, setJobs] = createSignal([]);
  const [schedules, setSchedules] = createSignal([]);
  const [logs, setLogs] = createSignal('');
  const [error, setError] = createSignal(null);

  onMount(load);

  async function load() {
    setError(null);
    try {
      const [jobList, scheduleList] = await Promise.all([api.list(100), api.schedules()]);
      setJobs(jobList.items || []);
      setSchedules(scheduleList.items || []);
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function showLogs(job) {
    setError(null);
    try {
      const result = await api.logs(job.id, 200);
      setLogs((result.items || []).map((row) => `${row.ts} [${row.level}] ${row.message}`).join('\n') || 'No log entries.');
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function run(schedule) {
    setError(null);
    try {
      await api.runSchedule(schedule.id);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  return (
    <section class="page">
      <header class="page-head">
        <div>
          <h1>Operations</h1>
          <p class="muted">{jobs().length} jobs · {schedules().length} schedules</p>
        </div>
        <div class="controls">
          <button class="secondary" onClick={load}>Refresh</button>
        </div>
      </header>

      <Show when={error()}><p class="error">{error()}</p></Show>

      <Tabs tabs={TABS} active={tab()} onChange={setTab} />

      <Show when={tab() === 'jobs'}>
        <Show when={jobs().length > 0} fallback={<p class="muted">No jobs yet.</p>}>
          <div class="table-scroll">
            <table>
              <thead>
                <tr><th>Kind</th><th>Status</th><th class="narrow">Progress</th>
                    <th class="narrow">Failed</th><th>Created</th><th /></tr>
              </thead>
              <tbody>
                <For each={jobs()}>
                  {(job) => (
                    <tr>
                      <td class="mono small">{job.kind}</td>
                      <td><StatusBadge status={job.status} /></td>
                      <td class="narrow mono small">{job.completed + job.failed}/{job.total}</td>
                      <td class="narrow mono small">{job.failed}</td>
                      <td class="mono small">{job.created_at}</td>
                      <td class="actions">
                        <button class="tiny" onClick={() => showLogs(job)}>Log</button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
        <Show when={logs()}>
          <pre class="logs">{logs()}</pre>
        </Show>
      </Show>

      <Show when={tab() === 'schedules'}>
        <Show when={schedules().length > 0} fallback={<p class="muted">No schedules.</p>}>
          <div class="table-scroll">
            <table>
              <thead>
                <tr><th>Name</th><th>Cron</th><th class="narrow">Enabled</th>
                    <th>Last run</th><th class="narrow">Result</th><th /></tr>
              </thead>
              <tbody>
                <For each={schedules()}>
                  {(schedule) => (
                    <tr>
                      <td class="mono small">{schedule.name}</td>
                      <td class="mono small">{schedule.cron}</td>
                      <td class="narrow">{schedule.enabled ? 'yes' : 'no'}</td>
                      <td class="mono small">{schedule.last_run_at || 'never'}</td>
                      <td class="narrow">
                        <Show when={schedule.last_status} fallback={<span class="muted small">—</span>}>
                          <StatusBadge status={schedule.last_status} />
                        </Show>
                      </td>
                      <td class="actions">
                        <button class="tiny" onClick={() => run(schedule)}>Run now</button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Show>
    </section>
  );
}
