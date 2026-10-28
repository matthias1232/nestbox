//! Where a destination is registered: the registry's RDAP record, cached by host.
//!
//! The traffic view shows a country next to every destination. The only source
//! this control plane accepts is the registry that allocated the address: RDAP,
//! the registries' own protocol. There is no geolocation vendor, no API key, and
//! no third party in the request path.
//!
//! ## What the lookup does
//!
//! 1. The host is turned into an address. A literal address is used as it is; a
//!    name is resolved, because the registry answers about addresses, not names.
//! 2. The IANA RDAP bootstrap tells which registry is authoritative for that
//!    address, by longest matching prefix. The bootstrap is fetched once and
//!    kept for the life of the process.
//! 3. That registry is asked for the address, and the record's country and ASN
//!    are read by the pure parsers in `domain::geography`.
//! 4. The answer — including an answer that names nothing — is cached in
//!    `host_geography`, so the next page costs no request.
//!
//! ## Why it runs off the request path
//!
//! A traffic page renders rows now; a registry lookup is a network call to a
//! third party that may be slow. [`spawn_fill`] therefore fills the cache in the
//! background and the page shows what is known. The next refresh has the
//! country. That is the difference between a console that sometimes pauses for
//! ARIN and one that does not.
//!
//! ## What it does not claim
//!
//! RDAP says where an allocation is *registered*. A CDN address registered in
//! the US answers from Frankfurt. Every surface that shows this value says
//! "registered in", and this module never invents a country it was not given.

use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::Utc;

use crate::domain::geography::{asn_of, country_of, owner_of, HostGeography};
use crate::store::postgres::geography as store;

/// How long a cached record is trusted.
///
/// Allocations move rarely; a week is short enough that a moved network is fixed
/// by the next page and long enough that a busy console does not ask ARIN about
/// the same address every minute.
pub const MAX_AGE_DAYS: i64 = 7;

/// How long one registry may take before the lookup is abandoned.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);

/// How many hosts one background fill takes on.
///
/// A traffic page names at most a few dozen destinations; the cap keeps a single
/// page from turning into a hundred sequential registry calls.
pub const MAX_HOSTS_PER_FILL: usize = 12;

/// The IANA bootstrap, fetched once per version and kept for the process.
static IPV4_BOOTSTRAP: OnceLock<Vec<BootstrapService>> = OnceLock::new();
static IPV6_BOOTSTRAP: OnceLock<Vec<BootstrapService>> = OnceLock::new();

/// One bootstrap entry: a prefix and the registry that answers for it.
#[derive(Debug, Clone)]
struct BootstrapService {
    prefix: IpAddr,
    length: u8,
    url: String,
}

impl BootstrapService {
    /// Whether an address falls inside this entry's prefix.
    fn covers(&self, address: IpAddr) -> bool {
        match (self.prefix, address) {
            (IpAddr::V4(base), IpAddr::V4(address)) => {
                let shift = 32u32.saturating_sub(u32::from(self.length));
                let mask = if shift == 32 { 0 } else { u32::MAX << shift };
                (u32::from(base) & mask) == (u32::from(address) & mask)
            }
            (IpAddr::V6(base), IpAddr::V6(address)) => {
                let shift = 128u32.saturating_sub(u32::from(self.length));
                let mask = if shift == 128 {
                    0
                } else {
                    u128::MAX << shift
                };
                (u128::from(base) & mask) == (u128::from(address) & mask)
            }
            _ => false,
        }
    }
}

/// Fill the cache for these hosts in the background.
///
/// The caller is a read path: it has rows to draw and must not wait for a
/// registry. Failures are logged at debug and dropped, because a missing country
/// is a blank cell, not an error the operator has to act on.
pub fn spawn_fill(pool: sqlx::PgPool, hosts: Vec<String>) {
    if hosts.is_empty() {
        return;
    }
    tokio::spawn(async move {
        if let Err(error) = fill(&pool, &hosts).await {
            tracing::debug!(error = %error, "geography fill did not complete");
        }
    });
}

/// Look up every host that has no recent record, and store what the registry says.
///
/// Sequential on purpose: these are a handful of hosts and one shared registry
/// per address family, and a burst of parallel requests is how a small control
/// plane gets rate-limited out of the data it needs.
pub async fn fill(pool: &sqlx::PgPool, hosts: &[String]) -> Result<(), crate::error::StoreError> {
    let mut unique: Vec<String> = hosts
        .iter()
        .map(|host| host.trim().to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .collect();
    unique.sort();
    unique.dedup();
    unique.truncate(MAX_HOSTS_PER_FILL);
    if unique.is_empty() {
        return Ok(());
    }

    let due = store::due_for_lookup(pool, &unique, chrono::Duration::days(MAX_AGE_DAYS)).await?;
    if due.is_empty() {
        return Ok(());
    }

    let client = match reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("nestbox-control-plane/2 (+rdap)")
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::debug!(error = %error, "no HTTP client for the RDAP lookup");
            return Ok(());
        }
    };

    for host in due {
        let record = match lookup(&client, &host).await {
            Some(record) => record,
            None => HostGeography {
                host: host.clone(),
                country: None,
                asn: None,
                as_owner: None,
                checked_at: Utc::now(),
            },
        };
        if let Err(error) = store::upsert(pool, &record).await {
            tracing::debug!(%host, error = %error, "could not cache the geography record");
        }
    }
    Ok(())
}

/// One host's record, or `None` when no registry answered.
async fn lookup(client: &reqwest::Client, host: &str) -> Option<HostGeography> {
    let address = resolve(host).await?;
    let base = registry_for(client, address).await?;
    let url = format!("{base}ip/{address}");
    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        tracing::debug!(%url, status = %response.status(), "the registry refused the lookup");
        return None;
    }
    let record: serde_json::Value = response.json().await.ok()?;
    Some(HostGeography {
        host: host.to_string(),
        country: country_of(&record),
        asn: asn_of(&record),
        as_owner: owner_of(&record),
        checked_at: Utc::now(),
    })
}

/// The address a host names.
///
/// A literal address needs no resolver. A name is resolved through the system
/// resolver — the same one the sandbox's traffic used — and the first address is
/// taken, because the registry answer is per address and a name may hold several.
async fn resolve(host: &str) -> Option<IpAddr> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address);
    }
    let target = format!("{host}:443");
    let mut addresses = tokio::net::lookup_host(target).await.ok()?;
    addresses.next().map(|socket| socket.ip())
}

/// The registry's RDAP base URL for one address.
///
/// The bootstrap is IANA's own file and is fetched once; a failure to fetch it
/// means no lookup, rather than a guess at which registry to ask.
async fn registry_for(client: &reqwest::Client, address: IpAddr) -> Option<String> {
    let (cell, family) = match address {
        IpAddr::V4(_) => (&IPV4_BOOTSTRAP, "ipv4"),
        IpAddr::V6(_) => (&IPV6_BOOTSTRAP, "ipv6"),
    };
    if let Some(services) = cell.get() {
        return best_match(services, address);
    }
    let url = format!("https://data.iana.org/rdap/{family}.json");
    let response = client.get(&url).send().await.ok()?;
    let document: serde_json::Value = response.json().await.ok()?;
    let services = parse_bootstrap(&document);
    if services.is_empty() {
        return None;
    }
    let chosen = best_match(&services, address);
    let _ = cell.set(services);
    chosen
}

/// The registry whose prefix is the most specific for this address.
///
/// Longest prefix wins, because the bootstrap lists both a global block and
/// narrower delegations inside it, and the narrower one is the registry that
/// actually holds the record.
fn best_match(services: &[BootstrapService], address: IpAddr) -> Option<String> {
    services
        .iter()
        .filter(|service| service.covers(address))
        .max_by_key(|service| service.length)
        .map(|service| service.url.clone())
}

/// Parse IANA's RDAP bootstrap (`{"services": [[[prefixes], [urls]], …]}`).
fn parse_bootstrap(document: &serde_json::Value) -> Vec<BootstrapService> {
    let mut out = Vec::new();
    let Some(services) = document.get("services").and_then(|v| v.as_array()) else {
        return out;
    };
    for service in services {
        let Some(entry) = service.as_array() else {
            continue;
        };
        let Some(prefixes) = entry.first().and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(urls) = entry.get(1).and_then(|v| v.as_array()) else {
            continue;
        };
        // Only an https endpoint is used: an RDAP answer over plain http could be
        // rewritten in transit, and this value is shown as a fact.
        let Some(url) = urls
            .iter()
            .filter_map(|url| url.as_str())
            .find(|url| url.starts_with("https://"))
        else {
            continue;
        };
        for prefix in prefixes.iter().filter_map(|v| v.as_str()) {
            let Some((address, length)) = prefix.split_once('/') else {
                continue;
            };
            let (Ok(address), Ok(length)) =
                (address.parse::<IpAddr>(), length.trim().parse::<u8>())
            else {
                continue;
            };
            out.push(BootstrapService {
                prefix: address,
                length,
                url: format!("{}/", url.trim_end_matches('/')),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bootstrap() -> Vec<BootstrapService> {
        parse_bootstrap(&json!({
            "services": [
                [["0.0.0.0/0"], ["https://rdap.example.net/"]],
                [["8.8.8.0/24"], ["https://rdap.example.org"]],
                [["2606:4700::/32"], ["http://plain.example", "https://rdap.example.com/"]]
            ]
        }))
    }

    #[test]
    fn the_bootstrap_keeps_only_https_endpoints() {
        let services = bootstrap();
        let v6 = services
            .iter()
            .find(|service| service.length == 32)
            .expect("v6 entry");
        // The trailing slash is normalised in: lookups append `ip/<address>`.
        assert_eq!(v6.url, "https://rdap.example.com/");
    }

    #[test]
    fn the_most_specific_prefix_wins() {
        let services = bootstrap();
        let google = "8.8.8.8".parse().expect("address");
        assert_eq!(
            best_match(&services, google).as_deref(),
            Some("https://rdap.example.org/")
        );
        assert_eq!(
            best_match(&services, "1.1.1.1".parse().expect("address")).as_deref(),
            Some("https://rdap.example.net/")
        );
    }

    #[test]
    fn an_address_outside_every_prefix_has_no_registry() {
        let services = parse_bootstrap(&json!({
            "services": [[["10.0.0.0/8"], ["https://rdap.example.net/"]]]
        }));
        assert_eq!(best_match(&services, "192.0.2.1".parse().expect("address")), None);
    }

    #[test]
    fn a_malformed_bootstrap_yields_nothing_rather_than_a_guess() {
        assert!(parse_bootstrap(&json!({ "services": "nope" })).is_empty());
        assert!(parse_bootstrap(&json!({})).is_empty());
    }

    #[test]
    fn a_prefix_covers_its_own_network_and_not_its_neighbour() {
        let service = BootstrapService {
            prefix: "10.0.0.0".parse().expect("address"),
            length: 8,
            url: "https://rdap.example.net/".into(),
        };
        assert!(service.covers("10.255.0.1".parse().expect("address")));
        assert!(!service.covers("11.0.0.1".parse().expect("address")));
    }
}
