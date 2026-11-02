// What the content policy found in this sandbox, for its owner.
//
// The same table the administrator sees, narrowed to this sandbox. Collapsed by
// default: a finding is worth seeing when it happens, not on every visit.
import { Show, createSignal } from 'solid-js';
import PrivacyDlpSection from '../privacy/PrivacyDlpSection.jsx';

export default function PrivacyCard(props) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="card">
      <div class="controls">
        <b>Datenschutz-Funde</b>
        <span class="muted small">was die Prüfung in dieser Sandbox gefunden hat, maskiert</span>
        <button class="tiny secondary" onClick={() => setOpen(!open())}>{open() ? 'Ausblenden' : 'Anzeigen'}</button>
      </div>
      <Show
        when={open()}
        fallback={
          <p class="muted small">
            Funde werden nur maskiert gespeichert: der Wert selbst ist nicht gespeichert und für niemanden lesbar.
          </p>
        }
      >
        <PrivacyDlpSection sandboxId={props.sandboxId} />
      </Show>
    </section>
  );
}
