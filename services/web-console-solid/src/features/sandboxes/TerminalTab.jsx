// Terminal tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function TerminalTab(props) {
  return (
    <PendingPanel
      title="Terminal"
      text="A real shell over a WebSocket: xterm.js, resize, reconnect. The server records who opened it and for how long, never what was typed."
      endpoint="WS /api/v1/sandboxes/{id}/terminal"
      owner="Package 13 · feat/console-terminal. The tab receives the sandbox and a reload callback."
    />
  );
}
