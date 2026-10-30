// A horizontal bar list: one row per group, each bar scaled to the largest value
// in the set.
//
// The numbers arrive from the server; this component only draws them and emits
// a click. `dense` styles stay inline because the console has no shared bar-list
// class and this file must not add one to `styles/` (that is the integrator's
// call, and a feature-local look is enough for one screen).

import { For, Show } from 'solid-js';

const ROW = {
  display: 'grid',
  gridTemplateColumns: 'minmax(96px, 1.1fr) minmax(80px, 2fr) minmax(104px, auto)',
  gap: '8px',
  alignItems: 'center',
};

const LABEL = {
  display: 'flex',
  flexDirection: 'column',
  gap: '1px',
  minWidth: 0,
};

const ELLIPSIS = {
  whiteSpace: 'nowrap',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
};

const LINK = {
  padding: 0,
  textAlign: 'left',
  maxWidth: '100%',
  ...ELLIPSIS,
};

const TRACK = {
  height: '8px',
  background: 'var(--bg-inset)',
  border: '1px solid var(--border)',
  borderRadius: '999px',
  overflow: 'hidden',
};

const FILL = {
  height: '100%',
  background: 'var(--accent)',
  borderRadius: '999px',
};

const META = {
  textAlign: 'right',
  whiteSpace: 'nowrap',
};

export default function BarList(props) {
  const rows = () => props.rows || [];
  // A zero-wide set still needs a divisor; an empty bar for a real zero is the
  // honest drawing, not a full bar.
  const largest = () => Math.max(1, ...rows().map((row) => Number(row.value) || 0));
  const width = (value) => {
    const share = ((Number(value) || 0) / largest()) * 100;
    return `${Math.max(0, Math.min(100, share))}%`;
  };

  return (
    <Show
      when={rows().length}
      fallback={<p class="muted small">{props.empty || 'Nothing to show in this window.'}</p>}
    >
      <div style={{ display: 'grid', gap: '6px' }}>
        <For each={rows()}>
          {(row) => (
            <div style={ROW}>
              <span style={LABEL}>
                <Show
                  when={row.onSelect}
                  fallback={<span style={ELLIPSIS}>{row.label}</span>}
                >
                  <button
                    class="link"
                    style={LINK}
                    title={row.title || `Show only ${row.label}`}
                    onClick={row.onSelect}
                  >
                    {row.label}
                  </button>
                </Show>
                <Show when={row.note}>
                  <span class="muted small" style={ELLIPSIS}>{row.note}</span>
                </Show>
              </span>

              <div style={TRACK} title={row.title}>
                <div style={{ ...FILL, width: width(row.value) }} />
              </div>

              <span class="mono small" style={META}>{row.meta}</span>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
