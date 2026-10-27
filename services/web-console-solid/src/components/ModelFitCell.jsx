// The verdict cell: badge, the two bars that decided it, and the shortfall in
// plain words.
//
// Nothing here is inferred: the badge is `fit.fit`, the bars are `fit.bars`, and
// the shortfall is the byte count the server subtracted. The text is a
// formatting of server numbers, not a second verdict.

import { Show } from 'solid-js';
import FitBadge from './FitBadge.jsx';
import ResourceBars, { gib } from './ResourceBars.jsx';

function shortfall(fit) {
  if (fit.vram_shortfall_bytes) return `short ${gib(fit.vram_shortfall_bytes)} of GPU memory`;
  if (fit.ram_shortfall_bytes) return `short ${gib(fit.ram_shortfall_bytes)} of RAM`;
  if (fit.required_cpu_cores && fit.available_cpu_cores
      && fit.required_cpu_cores > fit.available_cpu_cores) {
    return `needs ${fit.required_cpu_cores} cores, has ${fit.available_cpu_cores}`;
  }
  if (fit.fit === 'needs_unified_memory') return 'needs a unified-memory machine';
  if (fit.fit === 'no_gpu') return 'needs a GPU';
  return null;
}

export default function ModelFitCell(props) {
  const fit = () => props.fit || {};
  return (
    <div class="fitcell">
      <FitBadge fit={fit()} />
      <Show when={shortfall(fit())}>
        <span class="small bad-text">{shortfall(fit())}</span>
      </Show>
      <ResourceBars bars={fit().bars} compact />
    </div>
  );
}
