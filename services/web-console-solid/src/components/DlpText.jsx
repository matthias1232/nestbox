// A masked text with its findings marked: replaced, pseudonymised, blocked.
import { For, Show } from 'solid-js';
import { splitDlp } from './dlpText.js';
import DlpBadge from './DlpBadge.jsx';
import '../styles/dlp.css';

export default function DlpText(props) {
  const split = () => splitDlp(props.text, props.incidents || []);
  return (
    <span class={props.blocked ? 'dlp-blocked-text' : ''}>
      <For each={split().parts}>
        {(part) => (
          <Show when={part.kind !== 'text'} fallback={<>{part.text}</>}>
            <Show when={part.kind === 'replaced'}>
              <del class="dlp-del" title={`${part.incident.detector}: ersetzt`}>{part.incident.masked_value}</del>
              {' → '}
              <ins class="dlp-ins" onClick={() => props.onFinding?.(part.incident)}>Platzhalter</ins>
            </Show>
            <Show when={part.kind === 'pseudonymized'}>
              <span
                class="dlp-pseudo"
                title="Modell sah einen Platzhalter; der Agent bekommt den Originalwert zurück."
                onClick={() => props.onFinding?.(part.incident)}
              >
                {part.incident.masked_value}
              </span>
            </Show>
            <Show when={part.kind === 'blocked'}>
              <span class="dlp-del" onClick={() => props.onFinding?.(part.incident)}>{part.text}</span>
            </Show>
            <Show when={part.kind === 'unknown'}>
              <span class="muted">{part.text}</span>
            </Show>
          </Show>
        )}
      </For>
      <Show when={split().recorded.length}>
        <span class="dlp-recorded-chips">
          <For each={split().recorded}>
            {(incident) => (
              <span class="muted small" title="Nur protokolliert">
                {' '}· <DlpBadge state="allowed" /> {incident.detector}
              </span>
            )}
          </For>
        </span>
      </Show>
    </span>
  );
}
