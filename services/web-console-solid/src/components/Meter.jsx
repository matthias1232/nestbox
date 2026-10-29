// A labelled horizontal bar. `value` and `max` are numbers in the same unit;
// `promised` (optional) draws a marker for what has been promised, so "used" and
// "reserved" can be told apart.
export function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(Number(bytes))) return '—';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = Number(bytes);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export default function Meter(props) {
  const pct = () => (props.max > 0 ? Math.min(100, (props.value / props.max) * 100) : 0);
  const promised = () => (props.max > 0 && props.promised != null ? Math.min(100, (props.promised / props.max) * 100) : null);
  const tone = () => (pct() > 90 ? 'bad' : pct() > 75 ? 'warn' : 'ok');
  return (
    <div class="meter" title={props.title}>
      <div class="meter-head"><span>{props.label}</span><span class="mono small">{props.text}</span></div>
      <div class="meter-track">
        <div class={`meter-fill ${tone()}`} style={{ width: `${pct()}%` }} />
        {promised() != null && <div class="meter-mark" style={{ left: `${promised()}%` }} />}
      </div>
    </div>
  );
}
