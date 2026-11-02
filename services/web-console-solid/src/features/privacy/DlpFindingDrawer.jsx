// One finding in full: what fired, what happened, where the request was going,
// and the way to allow it on purpose.
//
// Stub: F-5 implements the detail view and the exception form.
import { Show } from 'solid-js';
import DlpBadge from '../../components/DlpBadge.jsx';

export default function DlpFindingDrawer(props) {
  const incident = () => props.incident?.incident || props.incident;
  return (
    <Show when={props.open}>
      <div class="card">
        <div class="controls">
          <DlpBadge state={incident()?.action} />
          <span class="mono small">{incident()?.detector}</span>
          <button class="tiny secondary" onClick={props.onClose}>Schließen</button>
        </div>
        <p class="muted small">{incident()?.masked_value || '—'}</p>
      </div>
    </Show>
  );
}
