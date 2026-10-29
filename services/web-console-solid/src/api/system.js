// The appliance's own containers: control API, gateway, runner, proxy, database.

import { request, query } from './client.js';

export function components() {
  return request('system/components');
}

// `service` is one of the names `components()` returns; the server refuses any other.
export function componentLogs(service, tail = 200) {
  return request(`system/components/${encodeURIComponent(service)}/logs${query({ tail })}`);
}
