// A node's budget: total, reserved, and what is left.
//
// The three numbers are the server's (`budget.ram_*`, `budget.vram_*`). The bars
// are one track per memory resource because total = reserved + free: the
// reserved part is drawn from the left in the muted colour and the free
// remainder after it, so "how much is left" is the length of the bright part
// rather than something to subtract by eye.
//
// A node nobody has measured has no capacity, and this component says so. It
// does not draw a zero-length bar for a fact nobody observed.

import { Show } from 'solid-js';
import { gib } from '../../components/ResourceBars.jsx';

export default function BudgetBars(props) {
  const budget = () => props.budget || {};
  const known = () => props.hardwareKnown === true;

  return (
    <Show
      when={known()}
      fallback={
        <p class="muted small node-unmeasured">
          Not measured yet. Nobody has probed this node, so it reports no capacity;
          the numbers here would be zeros, not facts.
        </p>
      }
    >
      <div class={props.compact ? 'budgetbars compact' : 'budgetbars'}>
        <MemoryBar
          label="System RAM"
          total={budget().ram_total_bytes}
          reserved={budget().ram_reserved_bytes}
          free={budget().ram_free_bytes}
        />
        <Show
          when={(budget().vram_total_bytes || 0) > 0}
          fallback={
            <div class="budgetbar">
              <span class="budgetbar-label">GPU memory</span>
              <span class="budgetbar-note muted small">none measured</span>
            </div>
          }
        >
          <MemoryBar
            label="GPU memory"
            total={budget().vram_total_bytes}
            reserved={budget().vram_reserved_bytes}
            free={budget().vram_free_bytes}
          />
        </Show>

        <div class="budgetmeta">
          <span>{count(budget().cpu_cores, 'core')}</span>
          <span>{count(budget().gpu_count, 'GPU')}</span>
          <span class="muted">
            {(budget().installs || 0) === 0
              ? 'no model install reserves memory'
              : `${budget().installs} model install${budget().installs === 1 ? '' : 's'} reserved`}
          </span>
        </div>

        <Show when={props.detectedAt}>
          <p class="muted small budget-when">
            measured {new Date(props.detectedAt).toLocaleString()}
          </p>
        </Show>
      </div>
    </Show>
  );
}

/**
 * One memory resource as three numbers and two segments.
 *
 * `reserved` can exceed `total` when an install was allowed against a larger
 * card, so the reserved segment is clamped at the full track and the free
 * segment disappears; that is the server's clamped truth, drawn.
 */
function MemoryBar(props) {
  const total = () => Math.max(0, props.total || 0);
  const reserved = () => Math.max(0, props.reserved || 0);
  const free = () => Math.max(0, props.free ?? total() - reserved());
  const reservedShare = () => (total() > 0 ? Math.min(100, (reserved() / total()) * 100) : 0);
  const freeShare = () =>
    total() > 0 ? Math.max(0, Math.min(100 - reservedShare(), (free() / total()) * 100)) : 0;

  return (
    <div class="budgetbar">
      <span class="budgetbar-label">{props.label}</span>
      <div
        class="budgetbar-track"
        title={`total ${gib(total())}, reserved ${gib(reserved())}, free ${gib(free())}`}
      >
        <div class="budgetbar-fill reserved" style={{ width: `${reservedShare()}%` }} />
        <div class="budgetbar-fill free" style={{ width: `${freeShare()}%` }} />
      </div>
      <span class="budgetbar-values mono">
        <b>{gib(free())} free</b>
        <span class="muted"> of {gib(total())} · {gib(reserved())} reserved</span>
      </span>
    </div>
  );
}

/** "8 cores", "1 GPU", "3 GPUs" — or a plain statement when nobody reported it. */
function count(value, unit) {
  const number = Number(value) || 0;
  if (number <= 0) return unit === 'GPU' ? 'GPUs not reported' : 'cores not reported';
  return `${number} ${unit}${number === 1 ? '' : 's'}`;
}
