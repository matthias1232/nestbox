// Logs tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function LogsTab(props) {
  return (
    <PendingPanel
      title="Logs"
      text="The container's recent output, with follow, and the image facts a report needs: digest, labels, baseline, recipe hash."
      endpoint="GET /api/v1/sandboxes/{id}/logs"
      owner="Package 16 · feat/console-inventory. The tab receives the sandbox and a reload callback."
    />
  );
}
