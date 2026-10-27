// Traffic tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

const TEXT =
  'The live table from one SSE stream, filtered by blocked/MCP/window, with ' +
  '"Allow this" producing a draft and "Allow everything for 15 min" opening a ' +
  'window with a countdown.';

export default function TrafficTab(props) {
  return (
    <PendingPanel
      title="Traffic"
      text={TEXT}
      endpoint="GET /api/v1/sandboxes/{id}/traffic · SSE /api/v1/telemetry/stream · POST /rule-drafts · POST /traffic-windows"
      owner="Package 11 · feat/console-traffic. The tab receives the sandbox and a reload callback."
    />
  );
}
