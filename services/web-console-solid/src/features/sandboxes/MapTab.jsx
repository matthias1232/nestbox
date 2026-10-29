// Topology tab of the sandbox Admin Center.
//
// The stack this sandbox belongs to, from you to the internet, with live state
// and a log behind every piece. `StackTopology.jsx` draws it; this file only
// hands it the sandbox, so the shell (`AdminCenter.jsx`) keeps its contract:
// a tab receives `sandbox`, `revision` and `reload`, and edits nothing else.

import StackTopology from './StackTopology.jsx';

export default function MapTab(props) {
  return <StackTopology sandbox={props.sandbox} revision={props.revision} />;
}
