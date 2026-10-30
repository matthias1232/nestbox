// Everything about one installed model that is not its reservation: the runtime
// flags it starts with, the log it writes, and the numbers it reports.
//
// The panel is deliberately one row wide and opened on demand: a page that
// fetched logs and metrics for every install would scrape servers nobody is
// looking at. Tuning saves through the same PATCH the RAM slider uses; a running
// model restarts with the new command, a stopped one only stores it.

import { For, Show, createMemo, createSignal, onMount } from 'solid-js';
import * as models from '../../api/models.js';

const CACHE_TYPES = ['f16', 'bf16', 'q8_0', 'q4_0', 'q4_1', 'iq4_nl', 'q5_0', 'q5_1'];

// llama.cpp metric names, as the runner returns them.
const PROMPT_TOKENS = 'llamacpp:prompt_tokens_total';
const OUTPUT_TOKENS = 'llamacpp:tokens_predicted_total';
const REQUESTS = 'llamacpp:requests_processing';
const CACHE_RATIO = 'llamacpp:kv_cache_usage_ratio';

function numberOrUndefined(value) {
  const text = String(value ?? '').trim();
  if (text === '') return undefined;
  const parsed = Number(text);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function triState(value) {
  if (value === undefined || value === null) return '';
  return value ? 'on' : 'off';
}

function fromTriState(value) {
  if (value === '') return undefined;
  return value === 'on';
}

/** A polyline for one series, scaled into the given box. */
function sparkline(values, width = 320, height = 72) {
  const points = values.filter((value) => Number.isFinite(value));
  if (points.length < 2) return '';
  const max = Math.max(...points);
  const min = Math.min(...points, 0);
  const span = max - min || 1;
  return points
    .map((value, index) => {
      const x = (index / (points.length - 1)) * width;
      const y = height - ((value - min) / span) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');
}

function formatNumber(value) {
  if (!Number.isFinite(value)) return '—';
  if (Math.abs(value) >= 1000) return value.toLocaleString();
  return String(Math.round(value * 100) / 100);
}

export default function ModelRuntimePanel(props) {
  const [tab, setTab] = createSignal('tune');
  const [notice, setNotice] = createSignal(null);
  const [error, setError] = createSignal(null);
  const [busy, setBusy] = createSignal(false);

  const [draft, setDraft] = createSignal({ ...(props.row.settings || {}) });
  const [logLines, setLogLines] = createSignal([]);
  const [logFilter, setLogFilter] = createSignal('all');
  const [metricAnswer, setMetricAnswer] = createSignal(null);

  const set = (key, value) => {
    const next = { ...draft() };
    if (value === undefined) delete next[key];
    else next[key] = value;
    setDraft(next);
  };

  async function save() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const answer = await models.updateInstall(props.row.model_key, {
        node: props.node,
        settings: draft(),
      });
      setNotice(
        `${props.row.model_key}: saved` +
        (answer.warnings?.length ? ` with ${answer.warnings.length} warning(s)` : '') +
        (answer.state === 'installing' ? '; the server is restarting' : '') + '.',
      );
      for (const warning of answer.warnings || []) setError(warning);
      props.onChanged?.();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  async function loadLogs() {
    setBusy(true);
    setError(null);
    try {
      const answer = await models.logs(props.row.model_key, props.node, 300);
      setLogLines(answer.lines || []);
    } catch (failure) {
      setError(failure.message);
      setLogLines([]);
    } finally {
      setBusy(false);
    }
  }

  async function loadMetrics() {
    setBusy(true);
    setError(null);
    try {
      setMetricAnswer(await models.metrics(props.row.model_key, props.node, 120));
    } catch (failure) {
      setError(failure.message);
      setMetricAnswer(null);
    } finally {
      setBusy(false);
    }
  }

  onMount(() => {
    // The row already carries its settings; no request until a tab asks.
  });

  const visibleLogs = createMemo(() => {
    const filter = logFilter();
    if (filter === 'all') return logLines();
    return logLines().filter((line) =>
      filter === 'error'
        ? /error|fatal|panic|exception/i.test(line)
        : /warn/i.test(line));
  });

  const series = createMemo(() => metricAnswer()?.series || []);
  const outputTokensPerSecond = createMemo(() => {
    const rows = series();
    const rates = [];
    for (let index = 1; index < rows.length; index += 1) {
      const before = rows[index - 1].metrics?.[OUTPUT_TOKENS];
      const after = rows[index].metrics?.[OUTPUT_TOKENS];
      const seconds = (new Date(rows[index].at) - new Date(rows[index - 1].at)) / 1000;
      if (Number.isFinite(before) && Number.isFinite(after) && seconds > 0) {
        rates.push(Math.max(0, (after - before) / seconds));
      }
    }
    return rates;
  });
  const cacheRatios = createMemo(() =>
    series().map((row) => Number(row.metrics?.[CACHE_RATIO])).filter(Number.isFinite));

  const current = createMemo(() => metricAnswer()?.current || {});

  return (
    <div class="runtime-panel">
      <div class="toolbar">
        <button class={tab() === 'tune' ? 'tiny' : 'tiny secondary'} onClick={() => setTab('tune')}>
          Tuning
        </button>
        <button class={tab() === 'logs' ? 'tiny' : 'tiny secondary'}
                onClick={() => { setTab('logs'); loadLogs(); }}>
          Logs
        </button>
        <button class={tab() === 'metrics' ? 'tiny' : 'tiny secondary'}
                onClick={() => { setTab('metrics'); loadMetrics(); }}>
          Metrics
        </button>
      </div>

      <Show when={error()}><p class="error small">{error()}</p></Show>
      <Show when={notice()}><p class="notice small">{notice()}</p></Show>

      <Show when={tab() === 'tune'}>
        <div class="runtime-grid">
          <label class="field">
            <span>Context size · {draft().context_size ?? 'image default'}</span>
            <input type="range" min="512" max="262144" step="512"
                   value={draft().context_size ?? 4096}
                   onInput={(event) => set('context_size', Number(event.currentTarget.value))} />
            <input type="number" min="128" max="1048576" step="128"
                   placeholder="default" value={draft().context_size ?? ''}
                   onInput={(event) => set('context_size', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>GPU layers · {draft().gpu_layers ?? 'image default'}</span>
            <input type="range" min="-1" max="99" step="1"
                   value={draft().gpu_layers ?? -1}
                   onInput={(event) => set('gpu_layers', Number(event.currentTarget.value))} />
            <input type="number" min="-1" max="999" step="1" placeholder="default"
                   value={draft().gpu_layers ?? ''}
                   onInput={(event) => set('gpu_layers', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Parallel requests</span>
            <input type="number" min="1" max="256" placeholder="default"
                   value={draft().parallel ?? ''}
                   onInput={(event) => set('parallel', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Batch size</span>
            <input type="number" min="32" max="16384" step="32" placeholder="default"
                   value={draft().batch_size ?? ''}
                   onInput={(event) => set('batch_size', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Micro-batch size</span>
            <input type="number" min="32" max="16384" step="32" placeholder="default"
                   value={draft().ubatch_size ?? ''}
                   onInput={(event) => set('ubatch_size', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Threads</span>
            <input type="number" min="1" max="1024" placeholder="node cores"
                   value={draft().threads ?? ''}
                   onInput={(event) => set('threads', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Threads (batch)</span>
            <input type="number" min="1" max="1024" placeholder="default"
                   value={draft().threads_batch ?? ''}
                   onInput={(event) => set('threads_batch', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>KV cache · keys</span>
            <select value={draft().cache_type_k ?? ''}
                    onChange={(event) => set('cache_type_k', event.currentTarget.value || undefined)}>
              <option value="">default</option>
              <For each={CACHE_TYPES}>{(type) => <option value={type}>{type}</option>}</For>
            </select>
          </label>

          <label class="field">
            <span>KV cache · values</span>
            <select value={draft().cache_type_v ?? ''}
                    onChange={(event) => set('cache_type_v', event.currentTarget.value || undefined)}>
              <option value="">default</option>
              <For each={CACHE_TYPES}>{(type) => <option value={type}>{type}</option>}</For>
            </select>
          </label>

          <label class="field">
            <span>Flash attention</span>
            <select value={triState(draft().flash_attention)}
                    onChange={(event) => set('flash_attention', fromTriState(event.currentTarget.value))}>
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>

          <label class="field">
            <span>mmap</span>
            <select value={triState(draft().mmap)}
                    onChange={(event) => set('mmap', fromTriState(event.currentTarget.value))}>
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>

          <label class="field">
            <span>mlock</span>
            <select value={triState(draft().mlock)}
                    onChange={(event) => set('mlock', fromTriState(event.currentTarget.value))}>
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>

          <label class="field">
            <span>KV cache on GPU</span>
            <select value={triState(draft().kv_offload)}
                    onChange={(event) => set('kv_offload', fromTriState(event.currentTarget.value))}>
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>

          <label class="field">
            <span>Tensor split</span>
            <input placeholder="e.g. 0.6,0.4" value={draft().tensor_split ?? ''}
                   onInput={(event) => set('tensor_split', event.currentTarget.value || undefined)} />
          </label>

          <label class="field">
            <span>Main GPU</span>
            <input type="number" min="0" max="15" placeholder="default"
                   value={draft().main_gpu ?? ''}
                   onInput={(event) => set('main_gpu', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Seed</span>
            <input type="number" placeholder="default" value={draft().seed ?? ''}
                   onInput={(event) => set('seed', numberOrUndefined(event.currentTarget.value))} />
          </label>

          <label class="field">
            <span>Chat template (jinja)</span>
            <select value={triState(draft().jinja)}
                    onChange={(event) => set('jinja', fromTriState(event.currentTarget.value))}>
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>

          <label class="field">
            <span>Metrics endpoint</span>
            <select value={triState(draft().metrics ?? true)}
                    onChange={(event) => set('metrics', fromTriState(event.currentTarget.value))}>
              <option value="">default (on)</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>
        </div>
        <div class="toolbar">
          <button class="tiny" disabled={busy()} onClick={save}>Save &amp; restart</button>
          <button class="tiny secondary" disabled={busy()}
                  onClick={() => { setDraft({ ...(props.row.settings || {}) }); setNotice(null); setError(null); }}>
            Reset
          </button>
          <span class="muted small">
            A running model restarts with the new command; a stopped one only stores it.
          </span>
        </div>
      </Show>

      <Show when={tab() === 'logs'}>
        <div class="toolbar">
          <label class="field">
            <span>Show</span>
            <select value={logFilter()} onChange={(event) => setLogFilter(event.currentTarget.value)}>
              <option value="all">all lines</option>
              <option value="warn">warnings</option>
              <option value="error">errors</option>
            </select>
          </label>
          <button class="tiny secondary" disabled={busy()} onClick={loadLogs}>Refresh</button>
          <span class="muted small">{visibleLogs().length} line(s)</span>
        </div>
        <pre class="log-view">{visibleLogs().length
          ? visibleLogs().join('\n')
          : 'No lines. A stopped model has no container to read.'}</pre>
      </Show>

      <Show when={tab() === 'metrics'}>
        <Show when={metricAnswer() && metricAnswer().available === false}>
          <p class="muted small">
            No metrics: {metricAnswer().reason}. Start the server with metrics enabled.
          </p>
        </Show>
        <Show when={metricAnswer() && metricAnswer().available}>
          <div class="toolbar">
            <span class="mono small">prompt {formatNumber(current()[PROMPT_TOKENS])} tok</span>
            <span class="mono small">output {formatNumber(current()[OUTPUT_TOKENS])} tok</span>
            <span class="mono small">in flight {formatNumber(current()[REQUESTS])}</span>
            <span class="mono small">KV cache {formatNumber((current()[CACHE_RATIO] || 0) * 100)}%</span>
            <button class="tiny secondary" disabled={busy()} onClick={loadMetrics}>Sample now</button>
          </div>
          <div class="charts">
            <figure>
              <figcaption class="muted small">Output tokens per second</figcaption>
              <svg viewBox="0 0 320 72" preserveAspectRatio="none" class="spark">
                <polyline fill="none" stroke="var(--accent, #d97706)" stroke-width="2"
                          points={sparkline(outputTokensPerSecond())} />
              </svg>
            </figure>
            <figure>
              <figcaption class="muted small">KV cache use</figcaption>
              <svg viewBox="0 0 320 72" preserveAspectRatio="none" class="spark">
                <polyline fill="none" stroke="var(--accent, #2563eb)" stroke-width="2"
                          points={sparkline(cacheRatios())} />
              </svg>
            </figure>
          </div>
          <p class="muted small">
            Consumers: {(metricAnswer().consumers || []).join(', ') || 'no sandbox selects this model'}
          </p>
        </Show>
      </Show>
    </div>
  );
}
