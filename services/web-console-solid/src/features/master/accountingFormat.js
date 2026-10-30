// Display rules for accounting numbers and outcomes.
//
// Pure functions, no signals and no JSX, so the rules that matter can be read
// without the screen around them. The one rule worth repeating: a missing price
// is `n/a`, never `0`, because `0` is indistinguishable from "free" and a total
// built from free-looking rows cannot be defended.

const MICROS_PER_UNIT = 1_000_000;

/// A whole count with thousands separators; anything that is not a number is
/// an em dash rather than a zero the server never said.
export function count(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value.toLocaleString() : '—';
}

/// Micros as an amount in the price list's currency.
///
/// The suffix is not printed: the server quotes micros against a price list, and
/// the console does not know which currency that list is written in. The page
/// says so once instead of inventing a symbol per row.
export function amount(micros) {
  if (typeof micros !== 'number' || !Number.isFinite(micros)) return null;
  const units = micros / MICROS_PER_UNIT;
  if (units === 0) return '0';
  if (units >= 1) return units.toFixed(2);
  if (units >= 0.01) return units.toFixed(4);
  return units.toFixed(6);
}

/// How many requests in a window carried no price.
///
/// The parallel backend names this `price_missing_requests`; the first
/// implementation names the priced ones instead. Both answer the same question:
/// a total that leaves requests out has to say how many it left out.
export function missingPriceCount(totals) {
  if (!totals) return 0;
  if (typeof totals.price_missing_requests === 'number') {
    return Math.max(0, totals.price_missing_requests);
  }
  if (typeof totals.priced_requests === 'number' && typeof totals.requests === 'number') {
    return Math.max(0, totals.requests - totals.priced_requests);
  }
  return 0;
}

/// The cost line for a window: the amount, or `n/a (no price)` with the reason.
///
/// `unknown` is not an error. It is the honest answer for a window whose traffic
/// has no price, and it is what keeps a partial sum from being read as a bill.
export function costLabel(totals) {
  if (!totals) return { unknown: true, text: '—', detail: null };
  const missing = missingPriceCount(totals);
  if (typeof totals.cost_micros !== 'number' || missing > 0) {
    const detail =
      missing > 0
        ? `${count(missing)} of ${count(totals.requests)} requests had no price; a total would leave them out.`
        : 'No request in this window had a price.';
    return { unknown: true, text: 'n/a (no price)', detail };
  }
  return {
    unknown: false,
    text: amount(totals.cost_micros),
    detail: 'In the currency of the model price list.',
  };
}

/// One row's cost: the amount, or `n/a` when that row had no price.
export function rowCost(row) {
  const micros = row?.cost_micros;
  if (typeof micros !== 'number' || !Number.isFinite(micros)) return 'n/a';
  return amount(micros);
}

/// How long a request took, in the unit the server reports.
export function latencyLabel(ms) {
  return typeof ms === 'number' && Number.isFinite(ms) ? `${count(ms)} ms` : '—';
}

/// A timestamp as the operator's own clock; an unparseable value is an em dash.
export function whenLabel(value) {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? '—' : date.toLocaleString();
}

/// The first eight characters of an id, for a name the server did not send.
export function shortId(value) {
  const text = String(value || '');
  if (!text) return '—';
  return text.length > 8 ? `${text.slice(0, 8)}…` : text;
}

/// The tone for an HTTP status: a 2xx succeeded, a 5xx is the provider's fault.
export function statusTone(status) {
  if (typeof status !== 'number') return 'muted';
  if (status >= 200 && status < 300) return 'ok';
  if (status >= 300 && status < 400) return 'info';
  if (status >= 400 && status < 500) return 'warn';
  if (status >= 500) return 'bad';
  return 'muted';
}

/// The actions the privacy layer records, with one sentence each.
///
/// `not_inspectable` is deliberately separate from `observed`: content the filter
/// could not read was not checked, and saying so is the difference between a
/// clean window and an unchecked one.
export const PRIVACY_ACTIONS = [
  {
    action: 'blocked',
    tone: 'bad',
    explanation: 'Refused before it left the sandbox; the provider never saw the request.',
  },
  {
    action: 'replaced',
    tone: 'warn',
    explanation: 'The matched content was rewritten, and the request continued with the replacement.',
  },
  {
    action: 'observed',
    tone: 'info',
    explanation: 'The request continued unchanged and the finding was recorded.',
  },
  {
    action: 'not_inspectable',
    tone: 'muted',
    explanation: 'The filter could not read the content, so nothing in it was checked.',
  },
];

/// The known actions plus any action the server named that the console does not
/// know. An unknown action is shown with that admission instead of a guessed
/// meaning, and its count is never dropped.
export function privacyRows(list) {
  const totals = new Map();
  for (const entry of list || []) {
    const action = String(entry?.action || '').trim() || 'unknown';
    const value = Number(entry?.count);
    totals.set(action, (totals.get(action) || 0) + (Number.isFinite(value) ? value : 0));
  }

  const known = new Set(PRIVACY_ACTIONS.map((item) => item.action));
  const rows = PRIVACY_ACTIONS.map((item) => ({
    ...item,
    count: totals.get(item.action) || 0,
  }));

  for (const [action, value] of totals) {
    if (known.has(action)) continue;
    rows.push({
      action,
      tone: 'muted',
      explanation: 'The server recorded this action; the console has no description for it.',
      count: value,
    });
  }
  return rows;
}

/// The tone for a privacy action, including the filter's own verb spellings.
export function privacyTone(action) {
  const key = String(action || '').toLowerCase();
  if (key === 'block' || key === 'blocked' || key === 'deny' || key === 'denied') return 'bad';
  if (key === 'replace' || key === 'replaced' || key === 'redact' || key === 'redacted') return 'warn';
  if (key === 'observe' || key === 'observed' || key === 'allow' || key === 'allowed') return 'info';
  return 'muted';
}
