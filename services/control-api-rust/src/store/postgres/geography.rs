//! The registry record of a destination address, cached by host.
//!
//! `host_geography` has no `environment_id`: a host's allocation is the same
//! fact for every tenant, exactly like `nodes`. The queries here therefore take
//! the pool rather than a `TenantScope`, and that is the whole reason they are
//! allowed to: reading this table cannot leak one tenant's data to another,
//! because the rows are not anyone's.
//!
//! The tenant boundary is kept where it belongs — in the traffic query that
//! joins this table onto rows it has already scoped by `environment_id` and
//! `sandbox_id`.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::domain::geography::HostGeography;
use crate::error::StoreError;

/// The cached records for these hosts, in whatever order the table returns.
///
/// An empty `hosts` is an empty answer, not a query: a page of traffic that
/// names no destination should not cost a round trip.
pub async fn cached(pool: &PgPool, hosts: &[String]) -> Result<Vec<HostGeography>, StoreError> {
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT host, country, asn, as_owner, checked_at
           FROM host_geography
          WHERE host = ANY($1)",
    )
    .bind(hosts)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_geography).collect()
}

/// Which of these hosts need a lookup, given how old a record may be.
///
/// A host with no record is always due. A host whose record is younger than
/// `max_age` is not, whatever it says: a record that names no country is an
/// answer, and re-asking it on every page is how a control plane becomes a
/// client of the registry.
pub async fn due_for_lookup(
    pool: &PgPool,
    hosts: &[String],
    max_age: chrono::Duration,
) -> Result<Vec<String>, StoreError> {
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let cutoff = Utc::now() - max_age;
    let rows = sqlx::query(
        "SELECT DISTINCT h.host
           FROM unnest($1::text[]) AS h(host)
           LEFT JOIN host_geography g ON g.host = h.host
          WHERE g.host IS NULL OR g.checked_at < $2",
    )
    .bind(hosts)
    .bind(cutoff)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| row.try_get("host").map_err(StoreError::from))
        .collect()
}

/// Store one record, replacing the previous one.
///
/// The record is the current answer, not a history: `checked_at` says when it
/// was asked, and a registry that moved an allocation is the newer truth.
pub async fn upsert(pool: &PgPool, entry: &HostGeography) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO host_geography (host, country, asn, as_owner, checked_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (host) DO UPDATE
            SET country = EXCLUDED.country,
                asn = EXCLUDED.asn,
                as_owner = EXCLUDED.as_owner,
                checked_at = EXCLUDED.checked_at",
    )
    .bind(&entry.host)
    .bind(entry.country.as_deref())
    .bind(entry.asn)
    .bind(entry.as_owner.as_deref())
    .bind(entry.checked_at)
    .execute(pool)
    .await?;
    Ok(())
}

fn row_to_geography(row: &sqlx::postgres::PgRow) -> Result<HostGeography, StoreError> {
    let checked_at: DateTime<Utc> = row.try_get("checked_at")?;
    Ok(HostGeography {
        host: row.try_get("host")?,
        country: row.try_get("country")?,
        asn: row.try_get("asn")?,
        as_owner: row.try_get("as_owner")?,
        checked_at,
    })
}
