// Topology tab of the sandbox Admin Center.
//
// The stack this sandbox belongs to, from you to the internet, with live state and a log behind
// every piece (`StackTopology.jsx`), and below it a walk through one real connection, step by
// step (`RequestJourney.jsx`), for learning how the pieces work together. The shell
// (`AdminCenter.jsx`) keeps its contract: a tab receives `sandbox`, `revision` and `reload`.

import StackTopology from './StackTopology.jsx';
import RequestJourney from './RequestJourney.jsx';

export default function MapTab(props) {
  return (
    <>
      <StackTopology sandbox={props.sandbox} revision={props.revision} />
      <RequestJourney sandbox={props.sandbox} />
    </>
  );
}
