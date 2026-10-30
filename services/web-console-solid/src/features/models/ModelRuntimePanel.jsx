// Everything about one installed model that is not its reservation: the runtime
// flags it starts with, the log it writes, and the numbers it reports.
//
// The tuning form is data-driven: FIELDS below is the single place that says
// what a control is, which range it accepts, what it does and when a value is
// recommended. Adding a flag to `ModelTuning` means adding one entry here, and
// the form, the help and the reset behaviour follow.

import { For, Show, createMemo, createSignal } from 'solid-js';
import * as models from '../../api/models.js';

const CACHE_TYPES = ['f16', 'bf16', 'q8_0', 'q4_0', 'q4_1', 'iq4_nl', 'q5_0', 'q5_1'];

// llama.cpp metric names, as the runner returns them.
const PROMPT_TOKENS = 'llamacpp:prompt_tokens_total';
const OUTPUT_TOKENS = 'llamacpp:tokens_predicted_total';
const REQUESTS = 'llamacpp:requests_processing';
const CACHE_RATIO = 'llamacpp:kv_cache_usage_ratio';

const onOff = [
  { value: '', label: 'image default' },
  { value: 'on', label: 'on' },
  { value: 'off', label: 'off' },
];

/**
 * The controls, in the order an operator thinks about them.
 *
 * `help` says what the flag does; `range` is what the server accepts;
 * `recommend` names the situations a value is good for. None of this is
 * enforced by the form — the server validates and warns, the text explains.
 */
const FIELDS = [
  {
    group: 'Context & generation',
    key: 'context_size', label: 'Context size', kind: 'range', min: 512, max: 262144, step: 512,
    unit: 'tokens',
    help: 'How many tokens the model can see at once: the prompt and the answer together. Every extra token costs KV-cache memory on the GPU.',
    range: '128 – 1,048,576 tokens',
    recommend: '4k–8k for chat, 16k–32k for coding agents, 64k+ only when the card has the memory for the cache.',
  },
  {
    group: 'Context & generation',
    key: 'n_predict', label: 'Max tokens per answer', kind: 'number', min: -1, max: 1048576,
    unit: 'tokens',
    help: 'Upper bound on the tokens one answer may produce. -1 lets the model write until the context is full.',
    range: '-1 or 1 – 1,048,576',
    recommend: '-1 for agents that decide their own length, 512–2048 to cap cost and latency.',
  },
  {
    group: 'Context & generation',
    key: 'n_keep', label: 'Tokens kept on shift', kind: 'number', min: -1, max: 1048576,
    help: 'When the context window slides, this many tokens from the start of the prompt stay. The system prompt survives, the middle is dropped.',
    range: '-1 or 0 – 1,048,576',
    recommend: 'Equal to the system prompt size (often 256–1024) for long chats.',
  },
  {
    group: 'Context & generation',
    key: 'context_shift', label: 'Context shifting', kind: 'bool',
    help: 'Lets the server slide the window when a conversation outgrows the context instead of refusing the request.',
    range: 'on / off',
    recommend: 'On for long chats; off when a truncated prompt would be worse than an error.',
  },
  {
    group: 'Context & generation',
    key: 'cache_reuse', label: 'KV cache reuse chunk', kind: 'number', min: 0, max: 65536,
    unit: 'tokens',
    help: 'Minimum chunk size reused from the prompt cache. Larger chunks reuse more but need a matching prefix.',
    range: '0 – 65,536',
    recommend: '256–512 for agent loops that resend a long, mostly identical prompt.',
  },
  {
    group: 'Context & generation',
    key: 'defrag_thold', label: 'KV defragmentation threshold', kind: 'number', min: -1, max: 1, step: 0.01,
    help: 'Defragments the KV cache when its fragmentation passes this ratio. -1 turns it off.',
    range: '-1 – 1',
    recommend: '0.1 for long-running servers with many short requests; -1 when latency matters more.',
  },

  {
    group: 'GPU & memory',
    key: 'gpu_layers', label: 'GPU layers', kind: 'range', min: -1, max: 99, step: 1,
    help: 'How many transformer layers are offloaded to the GPU. -1 offloads all; 0 keeps everything on the CPU.',
    range: '-1, or 0 – 999',
    recommend: '-1 when the model fits the VRAM; lower it step by step when the server runs out of memory.',
  },
  {
    group: 'GPU & memory',
    key: 'split_mode', label: 'Multi-GPU split', kind: 'select',
    options: [{ value: '', label: 'image default' }, { value: 'none', label: 'none (single GPU)' },
      { value: 'layer', label: 'layer' }, { value: 'row', label: 'row' }],
    help: 'How a model too large for one card is divided: whole layers per card, or rows within a layer.',
    range: 'none / layer / row',
    recommend: 'layer for most models; row only when a single layer does not fit one card.',
  },
  {
    group: 'GPU & memory',
    key: 'row_split', label: 'Row split', kind: 'bool',
    help: 'Splits tensors by rows across GPUs. Slower per token, but it can fit a model no layer split can.',
    range: 'on / off',
    recommend: 'Off unless a layer does not fit; then combine with split mode row.',
  },
  {
    group: 'GPU & memory',
    key: 'n_cpu_moe', label: 'MoE layers on CPU', kind: 'number', min: 0, max: 999,
    unit: 'layers',
    help: 'Keeps this many mixture-of-experts layers in system RAM instead of VRAM. Trades speed for fit.',
    range: '0 – 999',
    recommend: '0 with enough VRAM; raise it for Nemotron/Qwen MoE models that barely do not fit.',
  },
  {
    group: 'GPU & memory',
    key: 'main_gpu', label: 'Main GPU', kind: 'number', min: 0, max: 15,
    help: 'Index of the GPU that holds the first layers and the KV cache.',
    range: '0 – 15',
    recommend: '0 unless one card is faster or has more free memory.',
  },
  {
    group: 'GPU & memory',
    key: 'tensor_split', label: 'Tensor split', kind: 'text', placeholder: '0.6,0.4',
    help: 'Proportions per GPU, comma separated. 0.6,0.4 puts 60 % on the first card.',
    range: 'comma separated numbers, one per GPU',
    recommend: 'Leave empty for an even split; weight the card with more free VRAM higher.',
  },
  {
    group: 'GPU & memory',
    key: 'mmap', label: 'Memory-map the weights', kind: 'bool',
    help: 'Loads the model file through the page cache instead of reading it into anonymous memory. Saves RAM, can be slower on first use.',
    range: 'on / off',
    recommend: 'On for small machines; off when the model must not be evicted under memory pressure.',
  },
  {
    group: 'GPU & memory',
    key: 'mlock', label: 'Lock memory (mlock)', kind: 'bool',
    help: 'Pins the model in RAM so the kernel cannot swap it out. Needs a high memlock limit.',
    range: 'on / off',
    recommend: 'On for a dedicated server; off on a shared box where it would starve other work.',
  },
  {
    group: 'GPU & memory',
    key: 'kv_offload', label: 'KV cache on GPU', kind: 'bool',
    help: 'Keeps the KV cache in VRAM. Off moves it to system RAM: more context, much slower tokens.',
    range: 'on / off',
    recommend: 'On whenever the context fits; off only to stretch the context on a small card.',
  },

  {
    group: 'Batching & threads',
    key: 'parallel', label: 'Parallel requests', kind: 'number', min: 1, max: 256,
    help: 'How many requests the server processes at once. Each slot reserves its own KV cache.',
    range: '1 – 256',
    recommend: '1 for a single agent, 2–4 for a small team; memory grows roughly linearly.',
  },
  {
    group: 'Batching & threads',
    key: 'batch_size', label: 'Batch size', kind: 'number', min: 32, max: 16384, step: 32,
    unit: 'tokens',
    help: 'Tokens processed together while reading the prompt. Larger batches use more memory and raise prompt throughput.',
    range: '32 – 16,384',
    recommend: '2048 for long prompts, 512 for chat where latency matters.',
  },
  {
    group: 'Batching & threads',
    key: 'ubatch_size', label: 'Micro-batch size', kind: 'number', min: 32, max: 16384, step: 32,
    unit: 'tokens',
    help: 'The chunk the GPU actually computes in one step. Must not exceed the batch size.',
    range: '32 – 16,384, at most the batch size',
    recommend: '128–512 on consumer cards; raise it only if VRAM allows.',
  },
  {
    group: 'Batching & threads',
    key: 'threads', label: 'CPU threads', kind: 'number', min: 1, max: 1024,
    help: 'Threads for the CPU part of the work. Empty uses the node cores the controller computed.',
    range: '1 – 1024',
    recommend: 'Physical cores, not hyperthreads; leave empty unless the box is shared.',
  },
  {
    group: 'Batching & threads',
    key: 'threads_batch', label: 'Batch threads', kind: 'number', min: 1, max: 1024,
    help: 'Threads used while processing a prompt, separate from token generation.',
    range: '1 – 1024',
    recommend: 'Same as threads; more helps long prompts on CPU-heavy nodes.',
  },
  {
    group: 'Batching & threads',
    key: 'threads_http', label: 'HTTP worker threads', kind: 'number', min: 1, max: 256,
    help: 'Threads that accept and parse requests. Too few make the server look slow under concurrency.',
    range: '1 – 256',
    recommend: '4–8 for a handful of agents; raise it with parallel requests.',
  },
  {
    group: 'Batching & threads',
    key: 'timeout', label: 'Request timeout', kind: 'number', min: 1, max: 86400,
    unit: 'seconds',
    help: 'How long a request may run before the server closes it.',
    range: '1 – 86,400',
    recommend: '600 for chat, 3600 for long agent runs.',
  },
  {
    group: 'Batching & threads',
    key: 'no_warmup', label: 'Skip warm-up', kind: 'bool',
    help: 'Skips the warm-up run at start. The first real request pays the cost instead.',
    range: 'on / off',
    recommend: 'Off; on only when start time is more important than first-token latency.',
  },

  {
    group: 'KV cache',
    key: 'flash_attention', label: 'Flash attention', kind: 'bool',
    help: 'Faster attention with a smaller memory footprint. Supported by most modern cards and models.',
    range: 'on / off',
    recommend: 'On for NVIDIA cards with recent drivers; off only if the model produces garbage.',
  },
  {
    group: 'KV cache',
    key: 'cache_type_k', label: 'KV cache type · keys', kind: 'select',
    options: [{ value: '', label: 'image default' }, ...CACHE_TYPES.map((type) => ({ value: type, label: type }))],
    help: 'Quantises the keys of the KV cache. Smaller types fit more context into the same VRAM.',
    range: CACHE_TYPES.join(' / '),
    recommend: 'f16 for quality, q8_0 for a good trade, q4_0 when context matters more than precision.',
  },
  {
    group: 'KV cache',
    key: 'cache_type_v', label: 'KV cache type · values', kind: 'select',
    options: [{ value: '', label: 'image default' }, ...CACHE_TYPES.map((type) => ({ value: type, label: type }))],
    help: 'Quantises the values of the KV cache. Values are more sensitive than keys.',
    range: CACHE_TYPES.join(' / '),
    recommend: 'f16 or q8_0; avoid q4 for values unless the context must fit at any cost.',
  },

  {
    group: 'RoPE / YaRN (long context)',
    key: 'rope_scaling', label: 'RoPE scaling', kind: 'select',
    options: [{ value: '', label: 'image default' }, { value: 'none', label: 'none' },
      { value: 'linear', label: 'linear' }, { value: 'yarn', label: 'yarn' }],
    help: 'Stretches the position encoding beyond the context the model was trained on.',
    range: 'none / linear / yarn',
    recommend: 'none normally; yarn for a context well beyond the trained length; linear for a small stretch.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'rope_scale', label: 'RoPE scale', kind: 'number', min: 0.1, max: 64, step: 0.1,
    help: 'The factor the context is stretched by when linear scaling is used.',
    range: '0.1 – 64',
    recommend: '2 or 4 for twice or four times the trained context.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'rope_freq_base', label: 'RoPE frequency base', kind: 'number', min: 1, max: 10000000,
    help: 'The base frequency of the position encoding. Models trained with a different base need it here.',
    range: '1 – 10,000,000',
    recommend: 'Leave empty unless the model card names a base (often 10000 or 500000).',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'rope_freq_scale', label: 'RoPE frequency scale', kind: 'number', min: 0.01, max: 100, step: 0.01,
    help: 'Scales every frequency of the position encoding; the fine-grained form of rope scale.',
    range: '0.01 – 100',
    recommend: 'Empty normally; 0.5–0.25 to double or quadruple the context.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'yarn_orig_ctx', label: 'YaRN original context', kind: 'number', min: 128, max: 1048576,
    unit: 'tokens',
    help: 'The context the model was trained with; YaRN needs it to compute the correction.',
    range: '128 – 1,048,576',
    recommend: 'The trained context from the model card, e.g. 32768 or 131072.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'yarn_ext_factor', label: 'YaRN extrapolation factor', kind: 'number', min: -1, max: 100, step: 0.01,
    help: 'How far YaRN extrapolates instead of interpolating. -1 lets the server decide.',
    range: '-1 – 100',
    recommend: '-1 unless the model card suggests otherwise.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'yarn_attn_factor', label: 'YaRN attention factor', kind: 'number', min: 0, max: 100, step: 0.01,
    help: 'Scales attention scores under YaRN.',
    range: '0 – 100',
    recommend: '1.0 (default) unless quality degrades at long context.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'yarn_beta_fast', label: 'YaRN beta fast', kind: 'number', min: 0, max: 10000, step: 0.1,
    help: 'Correction strength for high-frequency dimensions.',
    range: '0 – 10,000',
    recommend: '32 (default) for most models.',
  },
  {
    group: 'RoPE / YaRN (long context)',
    key: 'yarn_beta_slow', label: 'YaRN beta slow', kind: 'number', min: 0, max: 10000, step: 0.1,
    help: 'Correction strength for low-frequency dimensions.',
    range: '0 – 10,000',
    recommend: '1 (default) for most models.',
  },

  {
    group: 'Sampling',
    key: 'seed', label: 'Seed', kind: 'number',
    help: 'Seed for sampling. The same seed and prompt produce the same answer.',
    range: 'any integer',
    recommend: 'Empty for variety; a fixed value for reproducible tests.',
  },
  {
    group: 'Sampling',
    key: 'temp', label: 'Temperature', kind: 'number', min: 0, max: 5, step: 0.05,
    help: 'How random the choice of the next token is. 0 is greedy, higher is more creative.',
    range: '0 – 5',
    recommend: '0–0.3 for code and extraction, 0.7–0.9 for writing, above 1.2 rarely useful.',
  },
  {
    group: 'Sampling',
    key: 'top_k', label: 'Top-K', kind: 'number', min: 0, max: 1000,
    help: 'Only the K most likely tokens are considered. 0 disables the filter.',
    range: '0 – 1000',
    recommend: '40 for general use, 0 to rely on top-p alone.',
  },
  {
    group: 'Sampling',
    key: 'top_p', label: 'Top-P', kind: 'number', min: 0, max: 1, step: 0.01,
    help: 'Keeps the smallest set of tokens whose probability adds up to P.',
    range: '0 – 1',
    recommend: '0.9–0.95 for chat, 0.5 for precise extraction.',
  },
  {
    group: 'Sampling',
    key: 'min_p', label: 'Min-P', kind: 'number', min: 0, max: 1, step: 0.01,
    help: 'Drops tokens below a fraction of the most likely token. A sharper alternative to top-p.',
    range: '0 – 1',
    recommend: '0.05 for creative work, 0.1 for balanced answers.',
  },
  {
    group: 'Sampling',
    key: 'typical_p', label: 'Typical-P', kind: 'number', min: 0, max: 1, step: 0.01,
    help: 'Keeps tokens whose probability is typical for the distribution. 1 disables it.',
    range: '0 – 1',
    recommend: '1 (off) unless you want less repetition in long prose.',
  },
  {
    group: 'Sampling',
    key: 'repeat_penalty', label: 'Repeat penalty', kind: 'number', min: 0, max: 10, step: 0.01,
    help: 'Penalises tokens that already appeared. Too high makes the model avoid necessary words.',
    range: '0 – 10',
    recommend: '1.0–1.15; above 1.3 damages code and names.',
  },
  {
    group: 'Sampling',
    key: 'presence_penalty', label: 'Presence penalty', kind: 'number', min: -2, max: 2, step: 0.05,
    help: 'Penalises any token that appeared at all, regardless of how often.',
    range: '-2 – 2',
    recommend: '0–0.5 to push a model toward new topics.',
  },
  {
    group: 'Sampling',
    key: 'frequency_penalty', label: 'Frequency penalty', kind: 'number', min: -2, max: 2, step: 0.05,
    help: 'Penalises tokens in proportion to how often they appeared.',
    range: '-2 – 2',
    recommend: '0–0.5 for long answers; negative values encourage repetition.',
  },
  {
    group: 'Sampling',
    key: 'mirostat', label: 'Mirostat', kind: 'select',
    options: [{ value: '', label: 'image default' }, { value: '0', label: '0 · off' },
      { value: '1', label: '1 · v1' }, { value: '2', label: '2 · v2' }],
    help: 'Adaptive sampling that targets a fixed perplexity instead of a fixed temperature.',
    range: '0 / 1 / 2',
    recommend: '0 normally; 2 when long outputs start to loop.',
  },
  {
    group: 'Sampling',
    key: 'mirostat_lr', label: 'Mirostat learning rate', kind: 'number', min: 0, max: 1, step: 0.01,
    help: 'How quickly Mirostat adapts to the target perplexity.',
    range: '0 – 1',
    recommend: '0.1 (default).',
  },
  {
    group: 'Sampling',
    key: 'mirostat_ent', label: 'Mirostat target entropy', kind: 'number', min: 0, max: 100, step: 0.1,
    help: 'The perplexity Mirostat aims for; higher means more creative.',
    range: '0 – 100',
    recommend: '5 (default); 3 for focused answers.',
  },

  {
    group: 'Serving',
    key: 'alias', label: 'Model alias', kind: 'text', placeholder: 'nemotron-local',
    help: 'The name the server reports in /v1/models, so a client can ask for a stable name.',
    range: 'non-empty, no spaces, at most 120 characters',
    recommend: 'A short stable name per deployment, e.g. coding-32k.',
  },
  {
    group: 'Serving',
    key: 'embeddings', label: 'Embeddings mode', kind: 'bool',
    help: 'Serves embeddings instead of chat completions. The model must support it.',
    range: 'on / off',
    recommend: 'On only for an embedding model; off for chat and agents.',
  },
  {
    group: 'Serving',
    key: 'pooling', label: 'Pooling', kind: 'select',
    options: [{ value: '', label: 'image default' }, { value: 'none', label: 'none' },
      { value: 'mean', label: 'mean' }, { value: 'cls', label: 'cls' },
      { value: 'last', label: 'last' }, { value: 'rank', label: 'rank' }],
    help: 'How token vectors are combined into one embedding. Only used in embeddings mode.',
    range: 'none / mean / cls / last / rank',
    recommend: 'mean for most embedding models; cls when the model card says so.',
  },
  {
    group: 'Serving',
    key: 'reasoning_format', label: 'Reasoning format', kind: 'select',
    options: [{ value: '', label: 'image default' }, { value: 'none', label: 'none' },
      { value: 'deepseek', label: 'deepseek' }, { value: 'deepseek-legacy', label: 'deepseek-legacy' }],
    help: 'Separates a reasoning model\'s thinking from its answer in the response.',
    range: 'none / deepseek / deepseek-legacy',
    recommend: 'deepseek for reasoning models; none for plain chat models.',
  },
  {
    group: 'Serving',
    key: 'slots', label: 'Slot monitoring', kind: 'bool',
    help: 'Exposes the slot endpoint that shows what each parallel slot is doing.',
    range: 'on / off',
    recommend: 'On while diagnosing concurrency; off in normal use.',
  },
  {
    group: 'Serving',
    key: 'verbose', label: 'Verbose logging', kind: 'bool',
    help: 'Writes every request detail to the container log. Useful, and loud.',
    range: 'on / off',
    recommend: 'Off; on temporarily when a request misbehaves.',
  },
  {
    group: 'Serving',
    key: 'metrics', label: 'Metrics endpoint', kind: 'bool',
    help: 'Exposes the numbers this page graphs (tokens, requests, cache use).',
    range: 'on / off',
    recommend: 'On. Without it the Metrics tab has nothing to show.',
  },
  {
    group: 'Serving',
    key: 'jinja', label: 'Chat template (jinja)', kind: 'bool',
    help: 'Uses the chat template stored in the model instead of the server\'s built-in one.',
    range: 'on / off',
    recommend: 'On for instruct models with their own template; off if the output looks malformed.',
  },
];

const GROUPS = [...new Set(FIELDS.map((field) => field.group))];

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
  const [help, setHelp] = createSignal('');

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

  const overridden = createMemo(() => Object.keys(draft()).length);

  async function save(settings = draft()) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const answer = await models.updateInstall(props.row.model_key, {
        node: props.node,
        settings,
      });
      setNotice(
        `${props.row.model_key}: saved` +
        (answer.warnings?.length ? ` with ${answer.warnings.length} warning(s)` : '') +
        (answer.state === 'installing' ? '; the server is restarting' : '') + '.',
      );
      for (const warning of answer.warnings || []) setError(warning);
      setDraft({ ...(settings || {}) });
      props.onChanged?.();
    } catch (failure) {
      setError(failure.message);
    } finally {
      setBusy(false);
    }
  }

  function resetAll() {
    setDraft({});
    setNotice('Every field is back to the image default. Save to apply it.');
    setError(null);
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

  function control(field) {
    const value = () => draft()[field.key];
    if (field.kind === 'range') {
      return (
        <div class="runtime-control">
          <input type="range" min={field.min} max={field.max} step={field.step || 1}
                 value={value() ?? field.min}
                 onInput={(event) => set(field.key, Number(event.currentTarget.value))} />
          <input type="number" min={field.min} max={field.max} step={field.step || 1}
                 placeholder="default" value={value() ?? ''}
                 onInput={(event) => set(field.key, numberOrUndefined(event.currentTarget.value))} />
        </div>
      );
    }
    if (field.kind === 'select') {
      return (
        <select value={String(value() ?? '')}
                onChange={(event) => {
                  const raw = event.currentTarget.value;
                  if (raw === '') set(field.key, undefined);
                  else if (field.key === 'mirostat') set(field.key, Number(raw));
                  else set(field.key, raw);
                }}>
          <For each={field.options}>{(option) => <option value={option.value}>{option.label}</option>}</For>
        </select>
      );
    }
    if (field.kind === 'bool') {
      return (
        <select value={triState(value())}
                onChange={(event) => set(field.key, fromTriState(event.currentTarget.value))}>
          <For each={onOff}>{(option) => <option value={option.value}>{option.label}</option>}</For>
        </select>
      );
    }
    if (field.kind === 'text') {
      return (
        <input placeholder={field.placeholder || 'default'} value={value() ?? ''}
               onInput={(event) => set(field.key, event.currentTarget.value || undefined)} />
      );
    }
    return (
      <input type="number" min={field.min} max={field.max} step={field.step || 'any'}
             placeholder="default" value={value() ?? ''}
             onInput={(event) => set(field.key, numberOrUndefined(event.currentTarget.value))} />
    );
  }

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
        <div class="toolbar">
          <button class="tiny" disabled={busy()} onClick={() => save()}>Save &amp; restart</button>
          <button class="tiny secondary" disabled={busy()}
                  onClick={() => { setDraft({ ...(props.row.settings || {}) }); setNotice(null); setError(null); }}>
            Discard changes
          </button>
          <button class="tiny secondary" disabled={busy()} onClick={resetAll}
                  title="Clear every field back to the image default">
            Reset to defaults
          </button>
          <button class="tiny secondary" disabled={busy() || overridden() === 0}
                  onClick={() => save({})}
                  title="Clear every field and restart with the image defaults">
            Reset &amp; restart
          </button>
          <span class="muted small">
            {overridden() === 0 ? 'all defaults' : `${overridden()} field(s) changed`}
          </span>
        </div>

        <For each={GROUPS}>
          {(group) => (
            <details class="tuning-group" open>
              <summary>{group}</summary>
              <div class="runtime-grid">
                <For each={FIELDS.filter((field) => field.group === group)}>
                  {(field) => (
                    <div class={`field runtime-field ${help() === field.key ? 'helping' : ''}`}>
                      <span class="runtime-label">
                        {field.label}
                        <button class="help-dot" type="button"
                                aria-label={`What ${field.label} does`}
                                onClick={() => setHelp(help() === field.key ? '' : field.key)}>
                          ?
                        </button>
                        <Show when={draft()[field.key] !== undefined}>
                          <button class="reset-dot" type="button"
                                  title="Back to the image default"
                                  onClick={() => set(field.key, undefined)}>
                            ↺
                          </button>
                        </Show>
                      </span>
                      {control(field)}
                      <Show when={help() === field.key}>
                        <div class="field-help">
                          <p>{field.help}</p>
                          <p><b>Range:</b> {field.range}</p>
                          <p><b>Recommended:</b> {field.recommend}</p>
                        </div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </details>
          )}
        </For>
        <p class="muted small">
          A running model restarts with the new command; a stopped one only stores it. Values the
          server refuses come back with the field named; expensive values are warnings, not errors.
        </p>
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
