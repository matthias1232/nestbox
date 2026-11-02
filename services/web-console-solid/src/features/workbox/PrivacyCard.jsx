// What the DLP layer found in this sandbox, for its owner.
//
// The same numbers the admin sees on the Content privacy page, narrowed to this
// sandbox: findings grouped by detector, and the masked excerpts. Collapsed by
// default: a finding is worth seeing when it happens, not on every visit.
import { Show, createSignal } from 'solid-js';
import { DlpDashboard, DlpLog } from '../privacy/DlpPanels.jsx';

export default function PrivacyCard(props) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="card">
      <div class="controls">
        <b>Privacy findings</b>
        <span class="muted small">what the detectors matched in this sandbox, masked</span>
        <button class="tiny secondary" onClick={() => setOpen(!open())}>{open() ? 'Hide' : 'Show'}</button>
      </div>
      <Show when={open()}>
        <DlpDashboard sandboxId={props.sandboxId} />
        <DlpLog sandboxId={props.sandboxId} limit={50} />
      </Show>
      <Show when={!open()}>
        <p class="muted small">
          Findings are kept masked: the value that matched is never stored, and no one —
          not an administrator — can read it back.
        </p>
      </Show>
    </section>
  );
}
