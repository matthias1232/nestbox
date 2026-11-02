// One thread of model calls, oldest first, as a messenger would show it.
//
// Contract (other files build against it): `turns` is an array of chat turns, a
// running turn carries `pending: true`. `title` is optional.
import { For, Show } from 'solid-js';
import DlpText from '../../components/DlpText.jsx';
import RouteChain from '../../components/RouteChain.jsx';
import '../../styles/dlp.css';

export default function ChatThread(props) {
  return (
    <div class="chat-thread">
      <Show when={props.title}><h3>{props.title}</h3></Show>
      <For each={props.turns || []} fallback={<p class="muted small">Noch keine Anfrage.</p>}>
        {(turn) => (
          <article class="chat-turn">
            <div class="chat-bubble user">
              <DlpText text={turn.user_text || ''} incidents={turn.incidents || []} />
            </div>
            <div class={`chat-bubble model ${turn.delivered === false ? 'blocked' : ''}`}>
              <DlpText
                text={turn.assistant_text || ''}
                incidents={turn.incidents || []}
                blocked={turn.delivered === false}
              />
              <Show when={turn.delivered === false}>
                <p class="small">Blockiert – nicht zugestellt.</p>
                <RouteChain route={turn.route} compact />
              </Show>
            </div>
          </article>
        )}
      </For>
    </div>
  );
}
