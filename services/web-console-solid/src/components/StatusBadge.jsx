// A status label with a colour that means the same thing everywhere.
//
// The colour is derived from the status string, so a new status that nobody
// classified shows as neutral instead of accidentally looking healthy.

const TONES = {
  running: 'ok', healthy: 'ok', succeeded: 'ok', valid: 'ok', installed: 'ok',
  starting: 'warn', provisioning: 'warn', degraded: 'warn', pending: 'warn',
  updating: 'warn', expiring: 'warn', partial: 'warn',
  stopped: 'muted', unknown: 'muted', cancelled: 'muted', not_installed: 'muted',
  failed: 'bad', unhealthy: 'bad', offline: 'bad', expired: 'bad', denied: 'bad',
  denied_by_policy: 'bad',
};

export default function StatusBadge(props) {
  const tone = () => TONES[props.status] || 'muted';
  return (
    <span class={`badge ${tone()}`} title={props.title || props.status}>
      <i />{props.label || props.status}
    </span>
  );
}
