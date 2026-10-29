// What each field of a rule means, what it accepts and an example, in one place.
//
// The forms show these under their fields, so an operator learns the vocabulary where
// they use it. Kept as data so a test can assert that no field is left unexplained.

export const RULE_HELP = {
  name: 'A label you will recognise in the log and in lists. Example: "allow npm registry".',
  action:
    'allow lets matching connections through; deny blocks them. Everything not allowed is blocked anyway, so a deny is for carving an exception out of a broader allow.',
  enforcement:
    'enforced blocks a violation. audit lets it through and records it: use audit first to see what a new rule would block, then switch to enforced.',
  protocol:
    'What the connection speaks. rest/http: plain HTTP requests, so methods can be checked. websocket, graphql, mcp, jsonrpc: the supervisor reads those requests too. tcp: any other protocol, relayed untouched. "any" matches every protocol.',
  priority:
    'Lower numbers are read first. When an allow and a deny both match, the lower number wins. The default is 100; use 10 for an exception that must win.',
  conditions:
    'Every condition has to hold for the rule to match. Add one for the destination at least; a rule with the condition "any" matches everything.',
  method: 'Limits the rule to one HTTP method, such as GET (read) or POST (write). Leave empty for all methods.',
  binary:
    'The program that opens the connection, by its real path (not a symlink), for example /usr/bin/curl. Leave empty for any program. A script counts as its interpreter, e.g. /usr/bin/python3.12.',
  scope:
    'One policy: the rule applies to every sandbox that carries that policy. One sandbox: only that sandbox. A rule always belongs to one of the two.',
  sandbox: 'The sandbox this rule applies to. It is enforced within about a minute, without a restart.',
  policy: 'The policy this rule belongs to. Every sandbox assigned that policy gets the rule.',
};

export const MATCHER_HELP = {
  host: 'One exact name. Example: api.github.com. It does not match subdomains.',
  host_suffix: 'A domain and everything under it. Example: .npmjs.org matches registry.npmjs.org. Needs at least two labels.',
  cidr: 'An address range, for hosts addressed by IP. Example: 10.90.92.0/24.',
  port: 'One port or a range. Examples: 443, 8000-8100. A range may cover at most 64 ports.',
  protocol: 'The wire protocol, as listed under Protocol above. Example: https.',
  method: 'An HTTP method. Example: GET.',
  binary: 'A program path. Example: /usr/bin/curl.',
  any: 'No condition: the rule matches every connection. Use it only for a deny that blocks everything else.',
};

export const DNS_HELP = {
  action:
    'deny stops the name from resolving at all, so nothing can connect to it; allow lets it resolve. Without any DNS rule every name resolves (the connection rules still apply).',
  pattern:
    'An exact name (tracker.example.com) or a .suffix that covers a domain and everything under it (.tracker.example).',
  priority: 'Lower numbers are read first. The default is 100.',
  scope: 'One policy (every sandbox that carries it) or one sandbox.',
};
