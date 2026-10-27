// Processes tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function ProcessesTab(props) {
  return (
    <PendingPanel
      title="Processes"
      text="docker top, refreshed on demand and on a 5 s interval while the tab is open, sortable."
      endpoint="GET /api/v1/sandboxes/{id}/processes"
      owner="Package 16 · feat/console-inventory. The tab receives the sandbox and a reload callback."
    />
  );
}
