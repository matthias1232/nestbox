// Split a masked text into the parts a reader has to see differently.
//
// A placeholder is the only trace of a replaced value that exists, so the split
// is what turns "the text says <IBAN_faf7e1c0>" into "here a value was replaced".
export const PLACEHOLDER = /<([A-Z_]+)_([0-9a-f]{8})>|\[REDACTED\]/g;

export function splitDlp(text, incidents = []) {
  const source = text || '';
  const parts = [];
  const recorded = [];
  let last = 0;
  for (const match of source.matchAll(PLACEHOLDER)) {
    const at = match.index ?? 0;
    if (at > last) parts.push({ kind: 'text', text: source.slice(last, at) });
    const found = incidents.find((incident) => incident.replacement === match[0]);
    if (!found) parts.push({ kind: 'unknown', text: match[0] });
    else if (found.action === 'pseudonymized') {
      parts.push({ kind: 'pseudonymized', text: match[0], incident: found });
    } else if (found.action === 'blocked') {
      parts.push({ kind: 'blocked', text: match[0], incident: found });
    } else {
      parts.push({ kind: 'replaced', text: match[0], incident: found });
    }
    last = at + match[0].length;
  }
  if (last < source.length) parts.push({ kind: 'text', text: source.slice(last) });
  for (const incident of incidents) {
    if (incident.action === 'allowed') recorded.push(incident);
  }
  return { parts, recorded };
}
