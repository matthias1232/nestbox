// Inventory tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function InventoryTab(props) {
  return (
    <PendingPanel
      title="Inventory"
      text="OS, package list with search, agent versions, listening ports, mounts, and the names of the environment variables — never their values."
      endpoint="GET /api/v1/sandboxes/{id}/inventory"
      owner="Package 16 · feat/console-inventory. The tab receives the sandbox and a reload callback."
    />
  );
}
