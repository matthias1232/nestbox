// One badge per content-check state, in the same colours everywhere.
import '../styles/dlp.css';

const LABELS = {
  blocked: 'Blockiert',
  replaced: 'Ersetzt',
  redacted: 'Ersetzt',
  pseudonymized: 'Pseudonymisiert',
  allowed: 'Protokolliert',
  unavailable: 'Nicht geprüft',
  too_large: 'Zu groß',
  exempt: 'Ausgenommen',
  skipped: 'Ausgenommen',
  clean: 'Sauber',
};

const EXPLANATIONS = {
  blocked: 'Die Anfrage hat das Modell nie erreicht.',
  replaced: 'Ein Wert wurde vor dem Versand ersetzt.',
  pseudonymized: 'Das Modell sah einen Platzhalter, der Agent bekommt den Wert zurück.',
  allowed: 'Nur protokolliert: die Anfrage ging unverändert hinaus.',
  unavailable: 'Die Prüfung konnte nicht antworten; die Anfrage wurde verweigert.',
  too_large: 'Die Anfrage war größer als die Prüfung liest; sie wurde verweigert.',
};

export default function DlpBadge(props) {
  const state = () => String(props.state || 'clean');
  return (
    <span class="dlp-badge" data-state={state()} title={props.title || EXPLANATIONS[state()] || ''}>
      {LABELS[state()] || state()}
    </span>
  );
}
