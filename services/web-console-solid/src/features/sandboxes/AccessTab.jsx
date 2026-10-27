// Access tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function AccessTab(props) {
  return (
    <PendingPanel
      title="Access"
      text="Published ports with their dial address, the SSH line, the one-time key download, rotation, and copy-paste blocks for VS Code, Claude Code, Antigravity and OpenChamber."
      endpoint="GET /api/v1/sandboxes/{id}/ports · /ssh-key"
      owner="Package 14 · feat/console-access. The tab receives the sandbox and a reload callback."
    />
  );
}
