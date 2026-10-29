// The MCP server registry: which tool servers exist, and which sandboxes use them.

import { request } from './client.js';

export const list = () => request('mcp/servers');
export const create = (body) => request('mcp/servers', { method: 'POST', body });
export const update = (key, body) => request(`mcp/servers/${encodeURIComponent(key)}`, { method: 'PUT', body });
export const remove = (key) => request(`mcp/servers/${encodeURIComponent(key)}`, { method: 'DELETE' });
export const attach = (sandboxId, serverKey) =>
  request(`sandboxes/${sandboxId}/mcp/servers`, { method: 'POST', body: { server_key: serverKey, enabled: true } });
export const attached = (sandboxId) => request(`sandboxes/${sandboxId}/mcp/servers`);
export const detach = (sandboxId, serverKey) =>
  request(`sandboxes/${sandboxId}/mcp/servers/${encodeURIComponent(serverKey)}`, { method: 'DELETE' });
