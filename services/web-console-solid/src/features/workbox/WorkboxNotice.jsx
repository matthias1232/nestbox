// What an end user is told about who may read this sandbox's chats.
//
// The sentence follows the setting, not the role: the same person may open the
// same box on a day when an administrator may read it and on a day when they may
// not, and the notice has to say which day it is (ADR 0063).
import { Show, createResource } from 'solid-js';

import * as privacy from '../../api/privacy.js';

export default function WorkboxNotice(props) {
  // A failed read says nothing rather than the wrong thing: an end user must
  // not be told "only metadata" because a request happened to fail.
  const [settings] = createResource(
    () => props.sandboxId,
    (sandboxId) => privacy.settings(sandboxId).catch(() => null),
  );

  const adminChatView = () => settings()?.settings?.admin_chat_view;

  return (
    <Show when={settings()?.settings}>
      <Show
        when={adminChatView()}
        fallback={<p class="muted small">Administratoren sehen nur Metadaten, keine Chat-Inhalte.</p>}
      >
        <p class="notice small">
          Hinweis: Chats und Modellanfragen dieser Sandbox können von Administratoren
          eingesehen werden. Gefundene sensible Daten werden maskiert.
        </p>
      </Show>
    </Show>
  );
}
