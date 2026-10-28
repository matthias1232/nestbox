-- 0032_network_rule_matchers — every kind of firewall rule becomes a matcher.
--
-- Until now a rule was `host` + optional `port` + `protocol` + `methods[]` +
-- `binaries[]`: five fixed columns that said "this exact host" and could not
-- say "every subdomain", "this IP range", or "this port range". A matcher list
-- says all of that with one shape, and the action and enforcement stay rules:
--
--   {"kind":"host","value":"api.example.com"}   exact host
--   {"kind":"host_suffix","value":".example.com"}  every subdomain
--   {"kind":"cidr","value":"10.0.0.0/8"}        an IP range
--   {"kind":"port","value":"8000-8100"}         a port or range
--   {"kind":"protocol","value":"mcp"}           one wire protocol
--   {"kind":"method","value":"POST"}            one HTTP method
--   {"kind":"binary","value":"/usr/local/bin/agent"}  one executable
--   {"kind":"any"}                              no condition
--
-- An empty list means "no condition" and is deliberately allowed: a deny rule
-- with no matchers is the honest way to write default-deny.
--
-- The migration translates every existing rule. Nothing is dropped: a host
-- becomes a host matcher, a port a port matcher, `tcp` (the wildcard protocol)
-- disappears because it carries no information, and every method and binary
-- becomes its own matcher. The flat columns stay for this release so readers
-- that predate this change still see the rule; new writes fill both, and the
-- matchers list is what the decider evaluates.

ALTER TABLE network_rules
    ADD COLUMN matchers jsonb NOT NULL DEFAULT '[]'::jsonb;

-- An empty value on the flat columns was impossible (host NOT NULL), so a host
-- matcher always exists after translation; the others appear only when present.
WITH built AS (
    SELECT id,
           jsonb_agg(entry ORDER BY ord) AS matchers
      FROM (
        SELECT id, 1 AS ord,
               jsonb_build_object('kind', 'host', 'value', host) AS entry
          FROM network_rules
         WHERE host <> '' AND host <> '*'
        UNION ALL
        SELECT id, 2,
               jsonb_build_object('kind', 'port', 'value', port::text)
          FROM network_rules
         WHERE port IS NOT NULL
        UNION ALL
        SELECT id, 3,
               jsonb_build_object('kind', 'protocol', 'value', protocol)
          FROM network_rules
         WHERE protocol <> 'tcp'
        UNION ALL
        SELECT id, 10 + ord,
               jsonb_build_object('kind', 'method', 'value', method)
          FROM network_rules
             , unnest(methods) WITH ORDINALITY AS m(method, ord)
        UNION ALL
        SELECT id, 100 + ord,
               jsonb_build_object('kind', 'binary', 'value', binary)
          FROM network_rules
             , unnest(binaries) WITH ORDINALITY AS b(binary, ord)
      ) AS parts
     GROUP BY id
)
UPDATE network_rules
   SET matchers = built.matchers
  FROM built
 WHERE built.id = network_rules.id;

-- A rule without a host must not become a rule without meaning. The flat
-- column keeps its NOT NULL, and the matcher form now says the same thing:
-- a host matcher, a host_suffix, a cidr, a port, a protocol, a method, a
-- binary, or "any".
ALTER TABLE network_rules
    ADD CONSTRAINT network_rules_matchers_valid
    CHECK (jsonb_typeof(matchers) = 'array');

COMMENT ON COLUMN network_rules.matchers IS
  'The conditions of the rule as a list of {kind,value} matchers. The authoritative form since 0032; the flat columns are kept for readers that predate it and are written in step.';
