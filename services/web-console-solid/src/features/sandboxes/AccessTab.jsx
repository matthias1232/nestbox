// Access tab of the sandbox Admin Center: how to reach this sandbox.
//
// The content is `ConnectPanel`, shared with the Connect drawer in the list.

import ConnectPanel from './ConnectPanel.jsx';

export default function AccessTab(props) {
  return (
    <section class="access-tab">
      <ConnectPanel sandboxId={props.sandbox?.id || props.sandboxId} />
    </section>
  );
}
