//! Where a destination is registered, as the registries' own RDAP records say.
//!
//! The traffic view answers "where does this sandbox connect to", and the only
//! honest source for a country without a third-party geolocation service is the
//! registry that allocated the address. RDAP is that registry's protocol: ask it
//! for the address, read the record it returns.
//!
//! Two properties of this module matter more than its parsers:
//!
//! 1. **It says "registered in".** RDAP describes an allocation, not a machine.
//!    A CDN address registered in the US answers from Frankfurt, and the console
//!    must not pretend otherwise.
//! 2. **Nothing here guesses.** A record that names no country yields `None`,
//!    because an invented country on a security screen is worse than a blank.
//!
//! The parsers are pure functions of a JSON value, so the awkward shapes
//! registries actually return are testable without a network.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What the registry says about one host.
///
/// `country` and `asn` are both optional: a record that names neither is still
/// worth storing, because "we asked and it said nothing" stops the next page
/// from asking again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HostGeography {
    pub host: String,
    /// ISO 3166-1 alpha-2 from the record, upper-cased.
    pub country: Option<String>,
    pub asn: Option<i64>,
    /// The network's name as the registry writes it, for a tooltip.
    pub as_owner: Option<String>,
    pub checked_at: DateTime<Utc>,
}

/// The country the record names, if it names a two-letter one.
///
/// Three shapes are accepted because registries differ: a top-level `country`
/// (ARIN and RIPE network objects carry one), the country component of an
/// entity's `adr` vcard (the standard place), and a two-letter country name in
/// a remark. A full country name such as "United States" is *not* translated:
/// a lookup table in this layer would be a second source of truth that silently
/// disagrees with the registry.
pub fn country_of(record: &serde_json::Value) -> Option<String> {
    if let Some(country) = record.get("country").and_then(|v| v.as_str()) {
        if let Some(code) = two_letter(country) {
            return Some(code);
        }
    }

    for entity in entities(record) {
        for element in vcard_elements(entity) {
            let kind = element.first().and_then(|v| v.as_str());
            if kind != Some("adr") {
                continue;
            }
            // The `adr` value is seven components: po box, extended address,
            // street, locality, region, postal code, country.
            if let Some(parts) = element.get(3).and_then(|v| v.as_array()) {
                if let Some(code) = parts
                    .get(6)
                    .and_then(|v| v.as_str())
                    .and_then(two_letter)
                {
                    return Some(code);
                }
            }
        }
    }

    for remark in remarks(record) {
        // A remark such as "Country: DE" is common in RIPE records that lack the
        // structured field. Only the whole remark being a code, optionally
        // after a "Country:" label, is taken: prose such as "Registered in
        // Germany" holds two-letter words ("in") that are not countries.
        let trimmed = remark.trim();
        let value = match trimmed.split_once(':') {
            Some((label, rest)) if label.trim().eq_ignore_ascii_case("country") => rest,
            _ => trimmed,
        };
        if let Some(code) = two_letter(value) {
            return Some(code);
        }
    }
    None
}

/// The autonomous system the record names, if it names one.
///
/// Accepted in three shapes, in the registry's own words: a top-level `asn`
/// (number or `AS15169`), an entity handle such as `AS15169`, or an `autnum`
/// entity. A record that names an AS in prose only is *not* searched, because
/// the first number in a remark is often a phone number.
pub fn asn_of(record: &serde_json::Value) -> Option<i64> {
    if let Some(asn) = record.get("asn").and_then(parse_asn) {
        return Some(asn);
    }
    for entity in entities(record) {
        if let Some(handle) = entity.get("handle").and_then(|v| v.as_str()) {
            if let Some(asn) = parse_asn(&serde_json::Value::String(handle.to_string())) {
                return Some(asn);
            }
        }
    }
    None
}

/// The network's name, for the tooltip next to the country.
///
/// `name` first (the network object's own field), then the registrant entity's
/// formatted name. A handle is not a name and is not used as one.
pub fn owner_of(record: &serde_json::Value) -> Option<String> {
    if let Some(name) = record
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(name.to_string());
    }
    for entity in entities(record) {
        let roles = entity
            .get("roles")
            .and_then(|v| v.as_array())
            .map(|roles| {
                roles
                    .iter()
                    .filter_map(|r| r.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !roles.iter().any(|role| *role == "registrant") {
            continue;
        }
        for element in vcard_elements(entity) {
            if element.first().and_then(|v| v.as_str()) == Some("fn") {
                if let Some(name) = element.get(3).and_then(|v| v.as_str()) {
                    let name = name.trim();
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

/// A two-letter country code, upper-cased, or nothing.
fn two_letter(value: &str) -> Option<String> {
    let value = value.trim();
    if value.len() == 2 && value.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some(value.to_ascii_uppercase());
    }
    None
}

/// `AS15169` or `15169` as a number, inside the 32-bit ASN range.
fn parse_asn(value: &serde_json::Value) -> Option<i64> {
    let raw = match value {
        serde_json::Value::Number(number) => number.as_i64()?,
        serde_json::Value::String(text) => {
            let text = text.trim();
            // The prefix is required: a bare number in a handle or a string
            // is as likely to be a ticket or phone number as an ASN.
            let digits = text
                .strip_prefix("AS")
                .or_else(|| text.strip_prefix("as"))?;
            digits.parse::<i64>().ok()?
        }
        _ => return None,
    };
    (1..=4_294_967_295).contains(&raw).then_some(raw)
}

/// The record's entity list.
fn entities(record: &serde_json::Value) -> Vec<&serde_json::Value> {
    record
        .get("entities")
        .and_then(|v| v.as_array())
        .map(|entities| entities.iter().collect())
        .unwrap_or_default()
}

/// One entity's vcard properties.
///
/// A jCard is `["vcard", [ ["fn", {}, "text", "Name"], … ]]`; the property list
/// is the second element.
fn vcard_elements(entity: &serde_json::Value) -> Vec<&Vec<serde_json::Value>> {
    entity
        .get("vcardArray")
        .and_then(|v| v.as_array())
        .and_then(|v| v.get(1))
        .and_then(|v| v.as_array())
        .map(|properties| properties.iter().filter_map(|p| p.as_array()).collect())
        .unwrap_or_default()
}

/// The record's remark descriptions.
fn remarks(record: &serde_json::Value) -> Vec<&str> {
    record
        .get("remarks")
        .and_then(|v| v.as_array())
        .map(|remarks| {
            remarks
                .iter()
                .flat_map(|remark| {
                    remark
                        .get("description")
                        .and_then(|v| v.as_array())
                        .map(|lines| {
                            lines
                                .iter()
                                .filter_map(|line| line.as_str())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_top_level_country_is_read_and_upper_cased() {
        let record = json!({ "country": "de" });
        assert_eq!(country_of(&record), Some("DE".to_string()));
    }

    #[test]
    fn a_country_name_is_not_translated() {
        let record = json!({ "country": "Germany" });
        assert_eq!(country_of(&record), None);
    }

    #[test]
    fn the_vcard_address_country_is_read() {
        let record = json!({
            "entities": [{
                "roles": ["registrant"],
                "vcardArray": ["vcard", [
                    ["version", {}, "text", "4.0"],
                    ["adr", { "label": "Some Street" }, "text",
                     ["", "", "", "Amsterdam", "", "1000 AA", "NL"]]
                ]]
            }]
        });
        assert_eq!(country_of(&record), Some("NL".to_string()));
    }

    #[test]
    fn a_remark_is_the_last_resort_and_must_be_a_bare_code() {
        let named = json!({ "remarks": [{ "description": ["Country: CH"] }] });
        assert_eq!(country_of(&named), Some("CH".to_string()));

        let prose = json!({ "remarks": [{ "description": ["Registered in Germany by Example GmbH"] }] });
        assert_eq!(country_of(&prose), None);
    }

    #[test]
    fn an_entity_handle_names_the_autonomous_system() {
        let record = json!({ "entities": [{ "handle": "AS15169", "roles": ["registrant"] }] });
        assert_eq!(asn_of(&record), Some(15_169));

        let numbered = json!({ "asn": 3320 });
        assert_eq!(asn_of(&numbered), Some(3320));

        let prefixed = json!({ "asn": "AS64500" });
        assert_eq!(asn_of(&prefixed), Some(64_500));
    }

    #[test]
    fn a_number_that_cannot_be_an_asn_is_refused() {
        // 0 and above 2^32-1 are not ASNs; a phone number in a handle is not one
        // either, because a handle without the AS prefix is not read as an ASN.
        assert_eq!(asn_of(&json!({ "asn": 0 })), None);
        assert_eq!(asn_of(&json!({ "asn": 9_999_999_999i64 })), None);
        assert_eq!(asn_of(&json!({ "entities": [{ "handle": "15169" }] })), None);
    }

    #[test]
    fn the_owner_is_the_network_name_then_the_registrant() {
        assert_eq!(
            owner_of(&json!({ "name": "GOOGLE-CLOUD" })),
            Some("GOOGLE-CLOUD".to_string())
        );
        let record = json!({
            "entities": [{
                "roles": ["registrant"],
                "vcardArray": ["vcard", [["fn", {}, "text", "Example GmbH"]]]
            }]
        });
        assert_eq!(owner_of(&record), Some("Example GmbH".to_string()));
        assert_eq!(owner_of(&json!({ "handle": "NET-1" })), None);
    }
}
