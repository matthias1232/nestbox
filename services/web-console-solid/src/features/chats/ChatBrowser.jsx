// The chat list and the selected thread.
//
// Stub: F-3 implements the grouped list, the live merge and the thread. The
// contract is fixed so the live stream and the workbox can be built against it.
import ChatThread from './ChatThread.jsx';

export default function ChatBrowser(props) {
  return (
    <section class="tab-panel">
      <p class="muted small">
        Model calls of this sandbox appear here once one has been recorded.
      </p>
      <ChatThread turns={[]} title={props.sandboxId ? 'Verlauf' : 'Alle Chats'} />
    </section>
  );
}
