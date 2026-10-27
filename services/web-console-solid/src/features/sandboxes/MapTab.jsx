// Map tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function MapTab(props) {
  return (
    <PendingPanel
      title="Map"
      text="Sandbox → destination edges for the last five minutes, with request counts and a red edge when anything was blocked. Clicking an edge filters the traffic tab."
      endpoint="GET /api/v1/sandboxes/{id}/map"
      owner="Package 15 · feat/console-map-policies. The tab receives the sandbox and a reload callback."
    />
  );
}
