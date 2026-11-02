// A masked text with its findings marked: replaced, pseudonymised, blocked.
import { For, Show, createMemo } from 'solid-js';
import { splitDlp } from './dlpText.js';
import DlpBadge from './DlpBadge.jsx';
import '../styles/dlp.css';

/**
 * Draw a masked text and mark each finding where it happened.
 *
 * A replaced value shows as "original masked → placeholder", a pseudonymised
 * value keeps the placeholder, and a blocked text loses its emphasis. Findings
 * that were only logged appear as chips below the text. Every marking reports
 * its incident through `onFinding`.
 */
export default function DlpText(props) {
  const split = createMemo(() => splitDlp(props.text, props.incidents || []));
  const report = (incident) => () => props.onFinding?.(incident);

  return (
    <span class={props.blocked ? 'dlp-blocked-text' : ''}>
      <For each={split().parts}>
        {(part) => (
          <Show when={part.kind !== 'text'} fallback={<>{part.text}</>}>
            <Show when={part.kind === 'replaced' || part.kind === 'redacted'}>
              <del class="dlp-del" title={`${part.incident.detector}: ersetzt`}>
                {part.incident.masked_value || part.text}
              </del>
              {' → '}
              <ins class="dlp-ins dlp-finding" title={part.text} onClick={report(part.incident)}>
                Platzhalter
              </ins>
            </Show>
            <Show when={part.kind === 'pseudonymized'}>
              <span
                class="dlp-pseudo dlp-finding"
                title="Modell sah einen Platzhalter; der Agent bekommt den Originalwert zurück."
                onClick={report(part.incident)}
              >
                {part.text}
              </span>
            </Show>
            <Show when={part.kind === 'blocked'}>
              <span class="dlp-del dlp-finding" title="Blockiert" onClick={report(part.incident)}>
                {part.text}
              </span>
            </Show>
            <Show when={part.kind === 'unknown'}>
              <span class="muted" title="Platzhalter ohne aufgezeichnetes Vorkommnis">
                {part.text}
              </span>
            </Show>
          </Show>
        )}
      </For>
      <Show when={split().recorded.length}>
        <span class="dlp-recorded-chips">
          <For each={split().recorded}>
            {(incident) => (
              <span class="dlp-recorded-chip" title="Nur protokolliert, nicht verändert">
                <DlpBadge state="allowed" />
                <span>{incident.detector}</span>
                <Show when={incident.masked_value}>
                  <code class="mono small">{incident.masked_value}</code>
                </Show>
              </span>
            )}
          </For>
        </span>
      </Show>
    </span>
  );
}
