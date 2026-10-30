// The five things an agent can be seen doing.
//
// The list is closed on purpose: an unknown kind keeps its own name but a grey
// colour and a neutral label, so a new server-side kind is visible instead of
// silently falling out of the legend.

export const KINDS = [
  { id: 'dns', label: 'DNS' },
  { id: 'https', label: 'HTTPS' },
  { id: 'mcp', label: 'MCP' },
  { id: 'render', label: 'Render' },
  { id: 'model', label: 'Model' },
];

const KNOWN = new Set(KINDS.map((kind) => kind.id));

// A CSS class per kind; `kind-unknown` is the grey fallback.
export function kindClass(kind) {
  return KNOWN.has(kind) ? `kind-${kind}` : 'kind-unknown';
}

export function kindLabel(kind) {
  return KINDS.find((item) => item.id === kind)?.label || 'unknown';
}
