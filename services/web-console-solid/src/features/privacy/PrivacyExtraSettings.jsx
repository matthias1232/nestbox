// The two environment switches: may an owner allow exceptions, may an admin read chats.
//
// They are the environment default only. A sandbox inherits them, and a sandbox
// override that repeated them would look like a local decision when it is not,
// so the block is hidden as soon as a sandbox is selected.
import { Show } from 'solid-js';

import Switch from '../../components/Switch.jsx';

export default function PrivacyExtraSettings(props) {
  return (
    <Show when={!props.privacy.sandboxId()}>
      <div style="display:flex;flex-direction:column;gap:.6rem;margin-bottom:var(--space-3)">
        <Switch
          checked={props.privacy.draft().owners_may_grant_exceptions}
          disabled={!props.isAdmin}
          onChange={(value) => props.privacy.setOwnersMayGrantExceptions(value)}
          label="Eigentümer dürfen Ausnahmen für ihre Sandbox anlegen"
          title={props.isAdmin ? '' : 'Nur ein Administrator kann die Umgebungsvorgabe ändern.'}
        />
        <Switch
          checked={props.privacy.draft().admin_chat_view}
          disabled={!props.isAdmin}
          onChange={(value) => props.privacy.setAdminChatView(value)}
          label="Administratoren dürfen Chat-Inhalte lesen (Endnutzer sehen dazu einen Hinweis)"
          title={props.isAdmin ? '' : 'Nur ein Administrator kann die Umgebungsvorgabe ändern.'}
        />
      </div>
    </Show>
  );
}
