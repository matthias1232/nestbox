// Path-based navigation without a router library.
//
// The console keeps its view in a signal, but three views have a real address:
// the Admin Center (`/sandboxes/<id>`) and the Workbox with its pop-outs
// (`/box/<id>`, `/box/<id>/terminal/<tab>`, `/box/<id>/desktop`). Parsing the
// path here means one place decides what a URL means, and a unit test proves it.

/** The address of one sandbox's Workbox. */
export const boxPath = (id) => `/box/${id}`;
/** The address of the list of my sandboxes. */
export const myPath = () => '/my';
/** The address of one terminal tab in its own window. */
export const terminalPath = (id, tab) => `/box/${id}/terminal/${tab}`;
/** The address of the desktop in its own window. */
export const desktopPath = (id) => `/box/${id}/desktop`;

/** Push a path and let the shell re-read it, like a link followed. */
export function navigate(path) {
  if (window.location.pathname === path) return;
  window.history.pushState({}, '', path);
  window.dispatchEvent(new PopStateEvent('popstate'));
}

const UUID = '([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})';

const PATTERNS = [
  { kind: 'terminal', re: new RegExp(`^/box/${UUID}/terminal/([A-Za-z0-9_-]{1,64})/?$`) },
  { kind: 'desktop', re: new RegExp(`^/box/${UUID}/desktop/?$`) },
  { kind: 'box', re: new RegExp(`^/box/${UUID}/?$`) },
  { kind: 'my', re: /^\/my\/?$/ },
  { kind: 'admin', re: new RegExp(`^/sandboxes/${UUID}/?$`) },
];

/**
 * What a path asks for: `{ kind, id, tab }`.
 *
 * `kind` is `null` for every other path, which the shell reads as "a menu view".
 */
export function parsePath(pathname) {
  for (const pattern of PATTERNS) {
    const match = pattern.re.exec(pathname);
    if (match) return { kind: pattern.kind, id: match[1] || null, tab: match[2] || null };
  }
  return { kind: null, id: null, tab: null };
}
