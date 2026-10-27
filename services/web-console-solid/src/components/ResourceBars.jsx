// "What you have" against "what it needs", per resource.
//
// The numbers, the status, and the verdict all come from the server: this
// component only draws them. That is deliberate — a second opinion computed in
// the browser would eventually disagree with the install gate.

const GIB = 1024 ** 3;

export function gib(bytes, digits = 1) {
  if (bytes == null) return '—';
  const value = bytes / GIB;
  if (value >= 100) return `${Math.round(value)} GiB`;
  return `${value.toFixed(digits).replace(/\.0$/, '')} GiB`;
}

/** The natural unit of a resource: bytes for memory, a count for cores and GPUs. */
export function amount(resource, value) {
  if (value == null) return '—';
  if (resource === 'cpu_cores') return `${value} cores`;
  if (resource === 'gpus') return `${value}`;
  return gib(value);
}

const LABELS = {
  vram: 'GPU memory',
  unified_memory: 'Unified memory',
  ram: 'System RAM',
  cpu_cores: 'CPU cores',
  gpus: 'GPUs',
};

const STATUS_TONE = {
  ok: 'ok',
  tight: 'warn',
  short: 'bad',
  unknown: 'muted',
  not_required: 'muted',
};

/** A bar is "short" when there is less than the minimum. */
function tone(bar) {
  return STATUS_TONE[bar.status] || 'muted';
}

/**
 * The percentage widths for one bar: the scale is the largest of what the model
 * needs and what the machine has, so a machine with more than enough shows a
 * full "have" bar and a shorter "need" bar instead of both being clipped.
 */
function widths(bar) {
  const scale = Math.max(bar.available || 0, bar.recommended || 0, bar.minimum || 0, 1);
  return {
    have: Math.min(100, ((bar.available || 0) / scale) * 100),
    minimum: Math.min(100, ((bar.minimum || 0) / scale) * 100),
    recommended: Math.min(100, ((bar.recommended || 0) / scale) * 100),
  };
}

export default function ResourceBars(props) {
  // `compact` is the per-row form: one line per gating resource, no labels for
  // resources the model does not need.
  const compact = () => props.compact === true;
  const bars = () => (props.bars || []).filter((bar) => bar.status !== 'not_required');

  return (
    <div class={compact() ? 'resbars compact' : 'resbars'}>
      {bars().map((bar) => {
        const width = widths(bar);
        return (
          <div class={`resbar ${tone(bar)}`}>
            <span class="resbar-label">{LABELS[bar.resource] || bar.resource}</span>
            <div class="resbar-tracks" title={
              `have ${amount(bar.resource, bar.available)}, ` +
              `min ${amount(bar.resource, bar.minimum)}` +
              (bar.recommended ? `, recommended ${amount(bar.resource, bar.recommended)}` : '')
            }>
              <div class="track have">
                <div class="fill" style={{ width: `${width.have}%` }} />
              </div>
              <div class="track need">
                <div class="fill" style={{ width: `${width.minimum}%` }} />
                {bar.recommended
                  ? <div class="fill rec" style={{ width: `${width.recommended}%` }} />
                  : null}
              </div>
            </div>
            <span class="resbar-values mono">
              <b>{amount(bar.resource, bar.available)}</b>
              <span class="muted">/ {amount(bar.resource, bar.minimum)}</span>
            </span>
          </div>
        );
      })}
    </div>
  );
}
