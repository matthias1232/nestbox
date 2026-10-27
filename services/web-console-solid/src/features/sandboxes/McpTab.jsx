// MCP tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function McpTab(props) {
  return (
    <PendingPanel
      title="MCP"
      text="Which servers this sandbox has, which tools were discovered, and the last calls with redacted arguments. A stdio server is marked as not visible to the proxy, because it is not egress."
      endpoint="GET /api/v1/sandboxes/{id}/mcp/servers · GET /api/v1/sandboxes/{id}/mcp/calls"
      owner="Package 12 · feat/console-mcp. The tab receives the sandbox and a reload callback."
    />
  );
}
