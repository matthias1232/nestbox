-- 0034_host_geography — where a destination's address is registered.
--
-- The traffic view answers "where does this sandbox connect to", and the answer
-- needs a country. The country comes from the registry record of the address:
-- RDAP, which is the registries' own protocol (RFC 7480 and friends), asked once
-- per host and cached here. No geolocation service, no API key, no third party
-- in the request path.
--
-- What this is not: a claim about where a server physically stands. RDAP says
-- which registry allocated the address and which country the allocation is
-- registered in. A CDN endpoint registered in the US may answer from Frankfurt.
-- The console shows the value as "registered in", never as "located in", because
-- the difference is the difference between an observation and a guess.
--
-- The table has no `environment_id`: a host's registry record is the same fact
-- for every tenant, exactly like `nodes`. Tenant isolation therefore lives in
-- what the traffic query joins, not in this table.
--
-- `country` and `asn` are both nullable on purpose: a registry record that names
-- neither still records that it was asked, so the lookup is not repeated every
-- time a row is drawn.

CREATE TABLE host_geography (
    host       text PRIMARY KEY,
    -- ISO 3166-1 alpha-2, as the registry wrote it.
    country    text,
    -- The autonomous system, when the record names it.
    asn        bigint,
    -- The network's name as the registry writes it, for a tooltip.
    as_owner   text,
    checked_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT host_geography_host_not_empty CHECK (length(host) > 0),
    CONSTRAINT host_geography_country_shape CHECK (country IS NULL OR country ~ '^[A-Z]{2}$'),
    CONSTRAINT host_geography_asn_sane CHECK (asn IS NULL OR asn BETWEEN 1 AND 4294967295)
);

COMMENT ON TABLE host_geography IS
  'Where a destination address is registered, from its registry RDAP record. Global data, looked up once per host. Says "registered in", never "located in".';

-- The traffic read joins on host, so the primary key is the index it needs.
COMMENT ON COLUMN host_geography.checked_at IS
  'When the record was last asked for. A reader may re-ask an old record; a failed lookup is stored too, so a host with no record does not cost a request per page.';
