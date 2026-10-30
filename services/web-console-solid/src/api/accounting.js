// Accounting: what a request used, what it cost, and what a window shows.
//
// `cost_micros` is `null` when no price was known, never `0`: the screen says
// "n/a" instead of drawing a free-looking total. The query builder drops an
// empty sandbox id, so an organisation-wide read carries no filter at all and
// an omitted sandbox is never sent as the string "undefined".

import { query, request } from './client.js';

// The window, and optionally one sandbox inside it. `timeframe` is the server's
// spelling (`5m`, `1h`, `24h`, `7d`, `30d`); it does not accept another.
export const overview = ({ timeframe = '24h', sandbox_id } = {}) =>
  request(`accounting/overview${query({ timeframe, sandbox_id })}`);

// The newest rows of the same window. A separate read on purpose: a window with
// a million requests has a summary the console can draw and a list it cannot.
export const requests = ({ timeframe = '24h', sandbox_id, limit = 100 } = {}) =>
  request(`accounting/requests${query({ timeframe, sandbox_id, limit })}`);

// How long raw rows are kept. Readable by anyone who may read telemetry; the
// write below is the admin half.
export const settings = () => request('accounting/settings');
export const saveSettings = (body) => request('accounting/settings', { method: 'PUT', body });

// Admin: roll the window into the daily table, then delete raw rows older than
// `days`. The response says how many went.
export const purge = (days) => request('accounting/purge', { method: 'POST', body: { days } });
