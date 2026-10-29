// What each field of an MCP server means, and what it accepts, in one place.

export const MCP_HELP = {
  key: 'A short unique id: lowercase letters, digits and dashes. Example: github. It cannot be changed later.',
  name: 'What people see in lists. Example: GitHub.',
  transport:
    'http: the server answers HTTP requests, so every call crosses the sandbox boundary and shows up in the log and the traffic. sse: the same with server-sent events. stdio: the server runs as a program inside the sandbox; nothing crosses the boundary, so Nestbox cannot see or check its calls.',
  endpoint:
    'http and sse only: the URL of the server, for example https://api.githubcopilot.com/mcp/. Attaching it allows exactly this host and port for the sandbox, nothing else.',
  command: 'stdio only: the program to start, for example npx.',
  args: 'stdio only: its arguments, one per line. Example: -y then @modelcontextprotocol/server-filesystem.',
  auth_type:
    'none: no credential. token: a bearer token the sandbox gets in an environment variable. oauth: the server signs you in itself.',
  env_slots:
    'Names of environment variables the server needs, one per line. Only names: the value is set in the sandbox, never stored here. The first one is the token for auth type token. Example: GITHUB_PERSONAL_ACCESS_TOKEN.',
  packs: 'Which agents may use it, one pack key per line (claude-code, opencode, hermes …). Empty means every agent.',
  enabled: 'A disabled server stays in the registry but cannot be attached to new sandboxes.',
  description: 'One sentence on what the tools are for. Shown in the list.',
};
