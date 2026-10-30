// Formatting for the live page.
//
// Values that were not reported are rendered as `n/a`, never as `0`: a missing
// token count and a token count of zero are different statements, and only one
// of them is true here.

const COUNT = new Intl.NumberFormat('en-US');

export function formatCount(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 'n/a';
  return COUNT.format(value);
}

// Costs arrive in micro-USD (1_000_000 = $1) so the server never sends a float
// that loses precision in transit.
export function formatCost(micros) {
  if (typeof micros !== 'number' || !Number.isFinite(micros)) return 'n/a';
  if (micros === 0) return '$0';
  const usd = micros / 1_000_000;
  if (usd < 0.0001) return '<$0.0001';
  return `$${usd.toFixed(4)}`;
}

// Accept an ISO string, epoch milliseconds, or epoch seconds. A timestamp that
// cannot be read stays unformatted rather than becoming "Invalid Date".
export function toMillis(at) {
  if (at === null || at === undefined) return null;
  if (typeof at === 'number') {
    if (!Number.isFinite(at)) return null;
    return at < 100_000_000_000 ? at * 1000 : at;
  }
  const parsed = Date.parse(at);
  return Number.isNaN(parsed) ? null : parsed;
}

export function formatTime(at) {
  const millis = toMillis(at);
  if (millis === null) return '—';
  return new Date(millis).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

export function truncate(text, max) {
  const value = String(text ?? '');
  return value.length > max ? `${value.slice(0, max - 1)}…` : value;
}

// A target is usually a URL or a model route; the scheme and trailing slash add
// length but no information at node size.
export function shortenTarget(target, max = 42) {
  const value = String(target ?? '').replace(/^https?:\/\//, '').replace(/\/$/, '');
  return truncate(value, max);
}
