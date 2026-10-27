// Dashboard tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function DashboardTab(props) {
  return (
    <PendingPanel
      title="Dashboard"
      text="CPU, memory, network and disk for 1h/24h/7d, the top processes, the last blocks, the MCP servers, and when the last event arrived."
      endpoint="GET /api/v1/sandboxes/{id}/metrics · /processes · /mcp/servers"
      owner="Package 10 · feat/admin-console-shell. The tab receives the sandbox and a reload callback."
    />
  );
}
