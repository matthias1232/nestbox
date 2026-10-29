// A tab must never take the console down with it.
//
// Before this existed, one exception while a tab rendered (a missing browser
// API, a malformed runtime value) unmounted the whole sandbox view and left a
// blank page with no message. The operator saw "the page is broken" and had no
// way to tell which tab caused it.
//
// The boundary shows the failure inside the tab body, names the error, and
// offers a retry that remounts the tab. The header, the tab strip and every
// other tab keep working, so a broken panel is a broken panel, not a broken
// sandbox.
import { ErrorBoundary } from 'solid-js';

export default function TabBoundary(props) {
  return (
    <ErrorBoundary
      fallback={(error, reset) => (
        <div class="tab-boundary" role="alert">
          <p class="error">
            This tab failed to render: {error?.message || String(error)}
          </p>
          <p class="muted small">
            The rest of the sandbox view is still usable. Retry remounts only
            this tab; it does not touch the sandbox.
          </p>
          <button class="tiny" onClick={() => reset()}>Retry this tab</button>
        </div>
      )}
    >
      {props.children}
    </ErrorBoundary>
  );
}
