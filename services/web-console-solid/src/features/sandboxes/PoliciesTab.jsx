// Policies tab of the sandbox Admin Center.
//
// Skeleton: the panel says what belongs here and which package fills it. The
// shell (`AdminCenter.jsx`) and the tab list belong to the integrator, so a
// package replaces this file and touches nothing else.

import PendingPanel from '../../components/PendingPanel.jsx';

export default function PoliciesTab(props) {
  return (
    <PendingPanel
      title="Policies"
      text="The rules that actually apply here, their source policy, enforcement, priority, and whether the proxy is running the current rule set."
      endpoint="GET /api/v1/sandboxes/{id}/policies/effective"
      owner="Package 15 · feat/console-map-policies. The tab receives the sandbox and a reload callback."
    />
  );
}
