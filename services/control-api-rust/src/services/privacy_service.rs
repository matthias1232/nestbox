//! Content privacy (DLP) for sandbox chat content.
//!
//! # What this layer does, and what it refuses to do
//!
//! A sandbox's chat content is scanned for secrets and personal data before it
//! moves. What happens next is the policy's decision (`domain::privacy`), and
//! what was found is recorded as **a masked excerpt and a SHA-256 fingerprint**
//! — never as the value. That last rule is structural rather than a convention:
//! `privacy_incidents` has no column for the value, so no bug here can store one.
//!
//! # Three decisions worth knowing
//!
//! 1. **The scan is hand-written, not regex.** The control API has no `regex`
//!    dependency, and adding one to the deployment image for ten patterns is a
//!    worse trade than the byte scanners below. Each one validates a checksum
//!    where the format has one (Luhn for cards, mod-97 for IBANs) and requires a
//!    word boundary, so the false-positive rate is a property of the code and
//!    not of the input.
//! 2. **A detector claims its bytes.** A card number is not also a phone number
//!    and an email is not also a hostname: the detectors run in a fixed priority
//!    order and a later one cannot take a span an earlier one claimed. Two
//!    incidents for one secret would double the evidence and the noise.
//! 3. **`review` never refuses.** A reviewer rewrites; only `strict` blocks. The
//!    mode matrix is one function in the domain, and this service never
//!    re-derives it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use super::access_control::require;
use super::ServiceContext;
use crate::domain::auth::{Permission, Principal};
use crate::domain::privacy::{
    Action, Detector, Finding, IncidentAction, Marked, Outcome, PrivacyIncident, PrivacyMode,
    PrivacySettings, PrivacyUpdate, apply,
};
use crate::error::{ApiError, ErrorCode};
use crate::store::postgres::privacy as store;
use crate::store::TenantScope;

/// The largest piece of content one inspection accepts.
///
/// 64 KiB is one chat turn with room to spare; a larger body is a file, and a
/// file belongs in the media and attachment paths, where a different policy
/// applies.
const MAX_CONTENT: usize = 64 * 1024;

/// How many findings one inspection reports and records.
///
/// Bounded on purpose: a pasted log full of hostnames would otherwise write
/// thousands of incident rows and spend the request in the incident insert loop.
/// The cap is on the *whole* content, so the first matches are the ones recorded
/// — a caller that needs more inspects again with a narrower detector set.
const MAX_FINDINGS: usize = 100;

/// The largest incident page a caller may ask for.
const MAX_INCIDENTS: i64 = 500;

/// The default incident page.
const DEFAULT_INCIDENTS: i64 = 100;

/// Which way the content is travelling.
///
/// Outbound content is what the account is usually worried about (it leaves for
/// a provider); inbound content is what arrived from one. The direction does not
/// change the detectors, only the evidence: whether an incident records a cloud
/// delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Content coming into the sandbox, from a model or an operator.
    Inbound,
    /// Content leaving the sandbox, for a model or a provider.
    Outbound,
}

impl Direction {
    /// The wire and audit spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }

    /// Read the wire spelling; unknown input is `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inbound" => Some(Self::Inbound),
            "outbound" => Some(Self::Outbound),
            _ => None,
        }
    }
}

/// One finding as the console shows it: what kind, what it looked like, what
/// happened. The masked value is the only trace of the content.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FindingView {
    /// The detector's name, as the settings and the incident row spell it.
    pub detector: String,
    /// First and last characters of the value, stars between.
    pub masked_value: String,
    /// What happened: `blocked`, `redacted`, `replaced` or `allowed`.
    pub action: String,
}

/// The answer to one inspection.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InspectResult {
    /// False means the caller must not send the content on.
    pub permitted: bool,
    /// `allowed`, `replaced` or `blocked`.
    pub action: String,
    /// The content to send instead. Identical to the input when nothing was
    /// rewritten, and the redacted text when the request is blocked — a caller
    /// that ignores `permitted` still never forwards a value.
    pub transformed_content: String,
    pub findings: Vec<FindingView>,
}

/// The SHA-256 of one matched value, as lowercase hex.
///
/// This is the token the domain's placeholder is built from and the column the
/// incident stores: the same value always yields the same fingerprint, so a
/// repeat leaks the same key is visible, and nothing can be recovered from it.
pub fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A byte span one detector claims in the text.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

/// Whether two spans share at least one byte.
fn overlaps(left: Span, right: Span) -> bool {
    left.start < right.end && right.start < left.end
}

/// The order the detectors run in.
///
/// Specific formats come first: an IBAN contains a long digit run and two
/// letters, a card contains a digit run, an email contains a hostname. Running
/// the specific detector first is what keeps the general one from claiming part
/// of it.
const PRIORITY: [Detector; 10] = [
    Detector::Iban,
    Detector::CreditCards,
    Detector::ApiKeys,
    Detector::Credentials,
    Detector::EmailAddresses,
    Detector::IpAddresses,
    Detector::PhoneNumbers,
    Detector::Hostnames,
    Detector::PersonNames,
    Detector::FinancialData,
];

/// Find every value the given detectors recognise, without ever returning the
/// same value twice.
pub fn detect(text: &str, detectors: &[Detector]) -> Vec<Finding> {
    let bytes = text.as_bytes();
    let mut claimed: Vec<Span> = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    for detector in PRIORITY {
        if !detectors.contains(&detector) {
            continue;
        }
        for span in scan(text, detector) {
            if span.start >= span.end || span.end > bytes.len() {
                continue;
            }
            // A span is only usable when it starts and ends on a character
            // boundary; every scanner works on ASCII, so this is a guard rather
            // than an expectation.
            if !text.is_char_boundary(span.start) || !text.is_char_boundary(span.end) {
                continue;
            }
            if claimed.iter().any(|existing| overlaps(*existing, span)) {
                continue;
            }
            let value = &text[span.start..span.end];
            if findings
                .iter()
                .any(|finding| finding.detector == detector && finding.value == value)
            {
                claimed.push(span);
                continue;
            }
            claimed.push(span);
            findings.push(Finding::new(detector, value));
            if findings.len() >= MAX_FINDINGS {
                return findings;
            }
        }
    }
    findings
}

/// Run one detector's scanner.
fn scan(text: &str, detector: Detector) -> Vec<Span> {
    let bytes = text.as_bytes();
    match detector {
        Detector::Credentials => scan_credentials(text, bytes),
        Detector::ApiKeys => scan_api_keys(text, bytes),
        Detector::EmailAddresses => scan_emails(bytes),
        Detector::PersonNames => scan_person_names(bytes),
        Detector::IpAddresses => scan_ipv4(bytes),
        Detector::Hostnames => scan_hostnames(bytes),
        Detector::PhoneNumbers => scan_phones(bytes),
        Detector::Iban => scan_ibans(bytes),
        Detector::CreditCards => scan_cards(bytes),
        Detector::FinancialData => scan_financial(text),
    }
}

fn is_alnum(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

/// A word boundary on the left: the value does not continue a word.
fn boundary_before(bytes: &[u8], index: usize) -> bool {
    index == 0 || !is_alnum(bytes[index - 1])
}

/// A word boundary on the right.
fn boundary_after(bytes: &[u8], end: usize) -> bool {
    end >= bytes.len() || !is_alnum(bytes[end])
}

fn is_local_part_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_domain_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
}

/// `local@domain.tld`, with a real TLD and a boundary on both sides.
fn scan_emails(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'@' {
            continue;
        }
        let mut start = index;
        while start > 0 && is_local_part_byte(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = index + 1;
        while end < bytes.len() && is_domain_byte(bytes[end]) {
            end += 1;
        }
        while end > index + 1 && matches!(bytes[end - 1], b'.' | b'-' | b'_') {
            end -= 1;
        }
        if start == index || end <= index + 1 {
            continue;
        }
        let domain = &bytes[index + 1..end];
        let Some(dot) = domain.iter().rposition(|candidate| *candidate == b'.') else {
            continue;
        };
        if dot == 0 {
            continue;
        }
        let label = &domain[dot + 1..];
        if label.len() < 2 || label.len() > 24 || !label.iter().all(u8::is_ascii_alphabetic) {
            continue;
        }
        spans.push(Span { start, end });
    }
    spans
}

/// The end of an IPv4 address starting at `start`, or `None`.
fn ipv4_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut position = start;
    for octet in 0..4 {
        let begin = position;
        let mut value: u32 = 0;
        while position < bytes.len() && bytes[position].is_ascii_digit() && position - begin < 3 {
            value = value * 10 + u32::from(bytes[position] - b'0');
            position += 1;
        }
        if position == begin || value > 255 {
            return None;
        }
        if octet < 3 {
            if position >= bytes.len() || bytes[position] != b'.' {
                return None;
            }
            position += 1;
        }
    }
    // A trailing digit or dot means a longer dotted run, not an address.
    if position < bytes.len() && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'.')
    {
        return None;
    }
    Some(position)
}

/// Four octets, each 0–255, at a word boundary.
fn scan_ipv4(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() || !boundary_before(bytes, index) {
            index += 1;
            continue;
        }
        match ipv4_end(bytes, index) {
            Some(end) => {
                spans.push(Span { start: index, end });
                index = end;
            }
            None => {
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                }
            }
        }
    }
    spans
}

/// Whether an IBAN's mod-97 checksum matches.
///
/// The check is what separates an account number from "DE89" followed by a word:
/// the algorithm moves the country and check digits to the end, maps letters to
/// 10–35, and requires the whole number to be 1 modulo 97.
fn iban_checksum_ok(candidate: &[u8]) -> bool {
    let mut remainder: u32 = 0;
    for byte in candidate[4..].iter().chain(candidate[..4].iter()) {
        if byte.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(byte - b'0')) % 97;
        } else if byte.is_ascii_uppercase() {
            let value = u32::from(byte - b'A') + 10;
            remainder = (remainder * 100 + value) % 97;
        } else {
            return false;
        }
    }
    remainder == 1
}

/// A country code, two check digits, and 11–30 more characters that pass the
/// mod-97 check.
///
/// Grouped IBANs (`DE89 3704 …`) are not matched: a space inside the number
/// makes it indistinguishable from two numbers in a sentence, and a false
/// positive here refuses a legitimate request.
fn scan_ibans(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index + 4 <= bytes.len() {
        if !bytes[index].is_ascii_uppercase()
            || !boundary_before(bytes, index)
            || !bytes[index + 1].is_ascii_uppercase()
            || !bytes[index + 2].is_ascii_digit()
            || !bytes[index + 3].is_ascii_digit()
        {
            index += 1;
            continue;
        }
        let mut end = index + 4;
        while end < bytes.len() && bytes[end].is_ascii_alphanumeric() && end - index < 34 {
            end += 1;
        }
        let length = end - index;
        if length < 15
            || !boundary_after(bytes, end)
            || !iban_checksum_ok(&bytes[index..end])
        {
            index += 1;
            continue;
        }
        spans.push(Span { start: index, end });
        index = end;
    }
    spans
}

/// Whether a card number satisfies the Luhn check.
///
/// Separators are skipped, so `4111 1111 1111 1111` and `4111111111111111` agree.
fn luhn_ok(candidate: &[u8]) -> bool {
    let mut sum: u32 = 0;
    let mut double = false;
    for byte in candidate.iter().rev() {
        if !byte.is_ascii_digit() {
            continue;
        }
        let mut value = u32::from(byte - b'0');
        if double {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
        double = !double;
    }
    sum > 0 && sum % 10 == 0
}

/// 13–19 digits, optionally grouped, that pass the Luhn check.
fn scan_cards(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() || !boundary_before(bytes, index) {
            index += 1;
            continue;
        }
        let start = index;
        let mut position = index;
        let mut digits = 0usize;
        let mut last_digit_end = index;
        let mut previous_was_digit = false;
        while position < bytes.len() {
            let byte = bytes[position];
            if byte.is_ascii_digit() {
                digits += 1;
                position += 1;
                last_digit_end = position;
                previous_was_digit = true;
                if digits > 19 {
                    break;
                }
            } else if matches!(byte, b' ' | b'-')
                && previous_was_digit
                && position + 1 < bytes.len()
                && bytes[position + 1].is_ascii_digit()
            {
                position += 1;
                previous_was_digit = false;
            } else {
                break;
            }
        }
        let end = last_digit_end;
        if (13..=19).contains(&digits)
            && boundary_after(bytes, end)
            && luhn_ok(&bytes[start..end])
        {
            spans.push(Span { start, end });
            index = end;
        } else {
            index = start + 1;
        }
    }
    spans
}

/// A phone number: 7–15 digits, and a shape a bare number does not have.
///
/// Either a leading `+` or at least one separator is required. A plain ten-digit
/// run is how an account number, a build id and a timestamp look too, and a
/// detector that flags all of those is turned off within a week.
fn scan_phones(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        let starts = (first.is_ascii_digit() || first == b'+' || first == b'(')
            && boundary_before(bytes, index);
        if !starts {
            index += 1;
            continue;
        }
        let start = index;
        let mut position = index;
        let mut digits = 0usize;
        let mut separators = 0usize;
        let mut last_digit_end = index;
        let leading_plus = first == b'+';
        if leading_plus {
            position += 1;
            if position >= bytes.len()
                || !(bytes[position].is_ascii_digit() || bytes[position] == b'(')
            {
                index += 1;
                continue;
            }
        }
        while position < bytes.len() {
            let byte = bytes[position];
            if byte.is_ascii_digit() {
                digits += 1;
                position += 1;
                last_digit_end = position;
                if digits > 15 {
                    break;
                }
            } else if matches!(byte, b' ' | b'-' | b'.')
                && position + 1 < bytes.len()
                && bytes[position + 1].is_ascii_digit()
            {
                separators += 1;
                position += 1;
            } else if matches!(byte, b'(' | b')') {
                separators += 1;
                position += 1;
            } else {
                break;
            }
        }
        let end = last_digit_end;
        if (7..=15).contains(&digits)
            && (leading_plus || separators > 0)
            && boundary_after(bytes, end)
        {
            spans.push(Span { start, end });
            index = end;
        } else {
            index = start + 1;
        }
    }
    spans
}

/// File extensions that look like a TLD and are not one.
///
/// Without this list every `report.txt` and `main.rs` in a pasted log becomes a
/// hostname. The list is a heuristic with a cost: an environment that really
/// talks to `.io`-style new TLDs is unaffected, one that talks to `.local`
/// internal names gets a match, and one that pastes `notes.md` does not.
const FILE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "json", "txt", "md", "yaml", "yml", "xml",
    "html", "htm", "pdf", "zip", "tar", "gz", "csv", "tsv", "log", "rs", "py", "js", "ts", "sh",
    "so", "dll", "exe", "bin", "img", "iso", "mp4", "webm", "wav", "mp3", "toml", "lock", "sql",
    "env", "cfg", "conf", "ini", "bak", "tmp", "dat",
];

fn is_host_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
}

/// Whether a dotted run is shaped like a hostname: two to eight labels, an
/// alphabetic last label, and not a dotted quad or a file name.
fn hostname_shape_ok(candidate: &[u8]) -> bool {
    let labels: Vec<&[u8]> = candidate.split(|byte| *byte == b'.').collect();
    if labels.len() < 2 || labels.len() > 8 {
        return false;
    }
    if labels
        .iter()
        .any(|label| label.is_empty() || label.len() > 63)
    {
        return false;
    }
    if labels
        .iter()
        .all(|label| label.iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    if labels
        .iter()
        .any(|label| label[0] == b'-' || label[label.len() - 1] == b'-')
    {
        return false;
    }
    let Some(tld) = labels.last() else {
        return false;
    };
    if tld.len() < 2 || tld.len() > 24 || !tld.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    let tld = String::from_utf8_lossy(tld).to_ascii_lowercase();
    !FILE_EXTENSIONS.contains(&tld.as_str())
}

/// A dotted name with a plausible last label.
fn scan_hostnames(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !is_host_byte(bytes[index]) || !boundary_before(bytes, index) {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index;
        while end < bytes.len() && is_host_byte(bytes[end]) {
            end += 1;
        }
        while end > start && matches!(bytes[end - 1], b'.' | b'-') {
            end -= 1;
        }
        if end > start && hostname_shape_ok(&bytes[start..end]) && boundary_after(bytes, end) {
            spans.push(Span { start, end });
            index = end;
        } else {
            index = start + 1;
        }
    }
    spans
}

/// Words that are capitalised for grammar rather than because they are a name.
const NAME_STOPWORDS: &[&str] = &[
    "The", "This", "That", "These", "Those", "There", "Then", "Dear", "Hello", "Hi", "Hey",
    "Regards", "Thanks", "Thank", "Note", "Warning", "Error", "Summary", "Chapter", "Section",
    "Step", "Example", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
    "Sunday", "January", "February", "March", "April", "May", "June", "July", "August",
    "September", "October", "November", "December",
];

/// Every run of alphabetic characters, with its span.
fn words_of(bytes: &[u8]) -> Vec<Span> {
    let mut words = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphabetic() || bytes[index] == b'\'')
        {
            index += 1;
        }
        words.push(Span { start, end: index });
    }
    words
}

/// `Marcus`, not `MARCUS` and not `marcus`.
fn is_capitalised_word(word: &[u8]) -> bool {
    if word.len() < 2 || word.len() > 24 {
        return false;
    }
    if !word[0].is_ascii_uppercase() {
        return false;
    }
    word[1..].iter().all(u8::is_ascii_lowercase)
}

/// Two capitalised words separated by one space.
///
/// This is the weakest detector and it says so: names have no checksum, and the
/// alternative — a model call — is a dependency this layer does not have. The
/// stopwords and the strict capitalisation rule are what keep it from flagging
/// every heading.
fn scan_person_names(bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let words = words_of(bytes);
    for pair in words.windows(2) {
        let first = pair[0];
        let second = pair[1];
        if &bytes[first.end..second.start] != b" " {
            continue;
        }
        let left = &bytes[first.start..first.end];
        let right = &bytes[second.start..second.end];
        if !is_capitalised_word(left) || !is_capitalised_word(right) {
            continue;
        }
        if NAME_STOPWORDS.iter().any(|word| word.as_bytes() == left)
            || NAME_STOPWORDS.iter().any(|word| word.as_bytes() == right)
        {
            continue;
        }
        spans.push(Span {
            start: first.start,
            end: second.end,
        });
    }
    spans
}

/// The currency symbols and codes an amount is written with.
const CURRENCY_SYMBOLS: [char; 4] = ['$', '€', '£', '¥'];
const CURRENCY_CODES: &[&str] = &["USD", "EUR", "GBP", "CHF", "JPY", "CAD", "AUD"];

/// The end of an amount: digits with thousands separators and a decimal part.
fn amount_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut position = start;
    let mut digits = 0usize;
    while position < bytes.len() {
        if bytes[position].is_ascii_digit() {
            digits += 1;
            position += 1;
        } else if matches!(bytes[position], b',' | b'.')
            && position + 1 < bytes.len()
            && bytes[position + 1].is_ascii_digit()
        {
            position += 1;
        } else {
            break;
        }
    }
    if digits == 0 {
        None
    } else {
        Some(position)
    }
}

/// A money amount: `$1,240.00`, `€900`, `1200 USD`.
fn scan_financial(text: &str) -> Vec<Span> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let offsets: Vec<(usize, char)> = text.char_indices().collect();
    for (index, (position, character)) in offsets.iter().enumerate() {
        if !CURRENCY_SYMBOLS.contains(character) {
            continue;
        }
        let Some((next, _)) = offsets.get(index + 1) else {
            continue;
        };
        if !bytes[*next].is_ascii_digit() {
            continue;
        }
        if let Some(end) = amount_end(bytes, *next) {
            spans.push(Span {
                start: *position,
                end,
            });
        }
    }
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() || !boundary_before(bytes, index) {
            index += 1;
            continue;
        }
        let Some(end) = amount_end(bytes, index) else {
            index += 1;
            continue;
        };
        let mut position = end;
        if position < bytes.len() && bytes[position] == b' ' {
            position += 1;
        }
        if position + 3 <= bytes.len() {
            let code = &bytes[position..position + 3];
            if CURRENCY_CODES.iter().any(|known| known.as_bytes() == code)
                && boundary_after(bytes, position + 3)
            {
                spans.push(Span {
                    start: index,
                    end: position + 3,
                });
            }
        }
        index = end;
    }
    spans
}

/// The keywords that introduce an assigned secret.
const CREDENTIAL_KEYWORDS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "client_secret",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "private_key",
    "token",
];

/// The characters a secret value may contain. Quotes, `&`, `;` and whitespace
/// end it, so a query string or a JSON body stops at its own delimiter.
fn is_secret_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b'-' | b'.' | b'!' | b'@' | b'#' | b'$' | b'%' | b'^' | b'*' | b'+' | b'/'
                | b'~' | b'?'
        )
}

/// The value of a `key = value` / `key: value` assignment, if there is one.
fn assignment_value(bytes: &[u8], mut position: usize) -> Option<Span> {
    while position < bytes.len() && bytes[position] == b' ' {
        position += 1;
    }
    if position >= bytes.len() || !matches!(bytes[position], b'=' | b':') {
        return None;
    }
    position += 1;
    while position < bytes.len()
        && matches!(bytes[position], b' ' | b'"' | b'\'' | b'`')
    {
        position += 1;
    }
    let start = position;
    while position < bytes.len() && is_secret_byte(bytes[position]) {
        position += 1;
    }
    if position - start < 4 {
        return None;
    }
    Some(Span { start, end: position })
}

/// A PEM private-key block, bounded so a huge paste cannot become one finding.
fn pem_block(text: &str) -> Option<Span> {
    let mut search = 0;
    while search < text.len() {
        let found = text[search..].find("-----BEGIN ")?;
        let start = search + found;
        let header_end = text[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(text.len());
        let header = &text[start..header_end];
        if header.contains("PRIVATE KEY") {
            let body_start = (header_end + 1).min(text.len());
            if let Some(offset) = text[body_start..].find("-----END ") {
                let marker = body_start + offset;
                let line_end = text[marker..]
                    .find('\n')
                    .map(|rest| marker + rest)
                    .unwrap_or(text.len());
                let end = line_end.min(start + 4096);
                if end > start {
                    return Some(Span { start, end });
                }
            }
        }
        search = header_end.max(start + 1);
    }
    None
}

/// An assigned password, token or key, and a PEM private-key block.
///
/// A blocker by default: these are the values that end an incident report.
fn scan_credentials(text: &str, bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    let lowered = text.to_ascii_lowercase();
    for keyword in CREDENTIAL_KEYWORDS {
        for (position, _) in lowered.match_indices(keyword) {
            if !boundary_before(bytes, position) {
                continue;
            }
            let after = position + keyword.len();
            // `passwords=`, `tokens=` and `secretive` are not assignments.
            if after < bytes.len() && is_alnum(bytes[after]) {
                continue;
            }
            if let Some(span) = assignment_value(bytes, after) {
                spans.push(span);
            }
        }
    }
    if let Some(span) = pem_block(text) {
        spans.push(span);
    }
    spans
}

/// The prefixes providers give their keys, so a key is recognised by its shape.
const API_KEY_PREFIXES: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AKIA",
    "ASIA",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "AIza",
    "glpat-",
];

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn is_base64url(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

/// The end of a JSON Web Token's three segments, starting after its `eyJ`.
fn jwt_end(bytes: &[u8], mut position: usize) -> Option<usize> {
    for segment in 0..3 {
        let start = position;
        while position < bytes.len() && is_base64url(bytes[position]) {
            position += 1;
        }
        if position - start < 6 {
            return None;
        }
        if segment == 2 {
            return Some(position);
        }
        if position >= bytes.len() || bytes[position] != b'.' {
            return None;
        }
        position += 1;
    }
    None
}

/// A prefixed provider key, or a JWT.
fn scan_api_keys(text: &str, bytes: &[u8]) -> Vec<Span> {
    let mut spans = Vec::new();
    for prefix in API_KEY_PREFIXES {
        for (position, matched) in text.match_indices(prefix) {
            if !boundary_before(bytes, position) {
                continue;
            }
            let mut end = position + matched.len();
            while end < bytes.len() && is_key_byte(bytes[end]) {
                end += 1;
            }
            if end - (position + matched.len()) < 12 {
                continue;
            }
            spans.push(Span { start: position, end });
        }
    }
    for (position, matched) in text.match_indices("eyJ") {
        if !boundary_before(bytes, position) {
            continue;
        }
        if let Some(end) = jwt_end(bytes, position + matched.len()) {
            spans.push(Span { start: position, end });
        }
    }
    spans
}

/// One incident the evaluation wants written.
struct IncidentDraft {
    detector: Detector,
    action: IncidentAction,
    masked_value: String,
    value_fingerprint: String,
    cloud_delivery: bool,
}

/// The whole answer of one inspection, before anything is written.
struct Evaluation {
    outcome: Outcome,
    incidents: Vec<IncidentDraft>,
}

/// Whether the content counts as a delivery to a cloud destination.
///
/// Three cases, and each is a decision:
///
/// * a blocked request delivered nothing, so `false`;
/// * `observe` delivered the content unchanged, so `true` — the incident exists
///   precisely to record that the payload left as it was;
/// * `review` and `strict` deliver a rewritten payload, so the destination and
///   the direction decide.
fn cloud_delivery(
    mode: PrivacyMode,
    direction: Direction,
    destination: Option<&str>,
    blocked: bool,
) -> bool {
    if blocked {
        return false;
    }
    match mode {
        PrivacyMode::Off => false,
        PrivacyMode::Observe => true,
        PrivacyMode::Review | PrivacyMode::Strict => {
            direction == Direction::Outbound && destination.is_some()
        }
    }
}

/// Scan and decide, without touching the database.
///
/// Pure so the whole mode matrix is a unit test, and so a caller can preview
/// what a policy would do to a document before saving it.
fn evaluate(
    settings: &PrivacySettings,
    direction: Direction,
    destination: Option<&str>,
    content: &str,
) -> Evaluation {
    if settings.mode == PrivacyMode::Off {
        return Evaluation {
            outcome: Outcome {
                action: Action::Allow,
                permitted: true,
                transformed: content.to_string(),
                decisions: Vec::new(),
            },
            incidents: Vec::new(),
        };
    }

    let detectors = settings.enabled_detectors();
    let marked: Vec<Marked> = detect(content, &detectors)
        .into_iter()
        .map(|finding| {
            let token = fingerprint(&finding.value);
            Marked::new(finding, token)
        })
        .collect();
    let outcome = apply(settings, &marked, content);
    let delivered = cloud_delivery(
        settings.mode,
        direction,
        destination,
        outcome.action == Action::Block,
    );
    let incidents = outcome
        .decisions
        .iter()
        .map(|decision| IncidentDraft {
            detector: decision.detector,
            action: decision.incident_action(),
            masked_value: decision.masked_value.clone(),
            value_fingerprint: decision.token.clone(),
            cloud_delivery: delivered,
        })
        .collect();

    Evaluation { outcome, incidents }
}

/// The settings that apply to a sandbox: its own row, else the environment
/// default, else the built-in strict policy.
///
/// The order is the product promise, and the last step is why an environment
/// that never opened the screen is still protected: absence of a row is the
/// strict policy, not "no policy".
async fn resolve_settings(
    scope: &mut TenantScope<'_>,
    sandbox_id: Option<Uuid>,
) -> Result<PrivacySettings, ApiError> {
    if let Some(sandbox_id) = sandbox_id {
        if let Some(settings) = store::settings_for(scope, Some(sandbox_id))
            .await
            .map_err(ApiError::from)?
        {
            return Ok(settings);
        }
    }
    if let Some(settings) = store::settings_for(scope, None)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(settings);
    }
    Ok(PrivacySettings::strict_default())
}

/// The settings a sandbox would use, and whether they are its own.
///
/// A returned `sandbox_id: None` means the row is the environment default the
/// sandbox inherits; `updated_at: None` means nothing was ever saved and the
/// built-in strict policy applies.
pub async fn effective_settings(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
) -> Result<PrivacySettings, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let settings = resolve_settings(&mut scope, sandbox_id).await?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(settings)
}

/// Save the policy for the environment (`sandbox_id: None`) or one sandbox.
///
/// `PolicyWrite`, not a sandbox permission: the environment row is what every
/// sandbox inherits, so writing it is a policy act. The body cannot name the
/// scope or the actor — both come from the path and the principal.
pub async fn set_settings(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Option<Uuid>,
    update: PrivacyUpdate,
) -> Result<PrivacySettings, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    update.validate().map_err(ApiError::validation)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    if let Some(sandbox_id) = sandbox_id {
        if !store::sandbox_exists(&mut scope, sandbox_id)
            .await
            .map_err(ApiError::from)?
        {
            return Err(ApiError::not_found(
                ErrorCode::SANDBOX_NOT_FOUND,
                "No such sandbox in this environment.",
            ));
        }
    }
    let mut settings = update.into_settings(sandbox_id);
    settings.updated_by = Some(principal.user_id);
    store::save_settings(&mut scope, &settings)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "privacy.settings",
            subject_type: Some(if sandbox_id.is_some() { "sandbox" } else { "environment" }),
            subject_id: Some(sandbox_id.unwrap_or(principal.environment_id)),
            detail: &json!({
                "mode": settings.mode.as_str(),
                "detectors": settings.detectors.iter().map(|detector| detector.as_str()).collect::<Vec<_>>(),
                "block_cloud_egress": settings.block_cloud_egress,
                "retention_days": settings.retention_days,
                "overrides": settings.actions.len(),
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let stored = store::settings_for(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    stored.ok_or_else(|| ApiError::internal("The privacy settings were not written."))
}

/// Drop a sandbox's override so it inherits the environment default again.
///
/// Returns whether an override existed: "inherit again" is idempotent, but the
/// audit trail should be able to say whether anything changed.
pub async fn clear_settings(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<bool, ApiError> {
    require(principal, Permission::PolicyWrite)?;
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    if !store::sandbox_exists(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
    {
        return Err(ApiError::not_found(
            ErrorCode::SANDBOX_NOT_FOUND,
            "No such sandbox in this environment.",
        ));
    }
    let removed = store::delete_override(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "privacy.inherit",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &json!({ "removed": removed }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(removed)
}

/// Inspect one piece of content and record what it found.
///
/// `TelemetryRead` rather than a sandbox permission: this is the path a gateway
/// calls on behalf of a sandbox's own traffic, and what it returns about the
/// content is evidence, not configuration. The content itself is the caller's —
/// it is inspected and handed back rewritten, never stored.
pub async fn inspect(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    direction: Direction,
    content: &str,
    destination: Option<&str>,
) -> Result<InspectResult, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    if content.len() > MAX_CONTENT {
        return Err(ApiError::validation(format!(
            "Content must be at most {MAX_CONTENT} bytes."
        )));
    }
    let destination = destination.filter(|value| !value.trim().is_empty());

    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    if !store::sandbox_exists(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
    {
        return Err(ApiError::not_found(
            ErrorCode::SANDBOX_NOT_FOUND,
            "No such sandbox in this environment.",
        ));
    }
    let settings = resolve_settings(&mut scope, Some(sandbox_id)).await?;
    let evaluation = evaluate(&settings, direction, destination, content);
    for incident in &evaluation.incidents {
        store::insert_incident(
            &mut scope,
            store::NewIncident {
                sandbox_id,
                detector: incident.detector,
                action: incident.action,
                severity: incident.action.severity(),
                masked_value: &incident.masked_value,
                value_fingerprint: &incident.value_fingerprint,
                destination,
                cloud_delivery: incident.cloud_delivery,
            },
        )
        .await
        .map_err(ApiError::from)?;
    }
    scope.commit().await.map_err(ApiError::from)?;

    Ok(view(evaluation))
}

/// The wire shape of one evaluation.
///
/// Split out of [`inspect`] so the mapping from a decision to a finding view —
/// which is what the console reads — is testable without a database.
fn view(evaluation: Evaluation) -> InspectResult {
    let findings: Vec<FindingView> = evaluation
        .outcome
        .decisions
        .iter()
        .map(|decision| FindingView {
            detector: decision.detector.as_str().to_string(),
            masked_value: decision.masked_value.clone(),
            action: decision.incident_action().as_str().to_string(),
        })
        .collect();
    InspectResult {
        permitted: evaluation.outcome.permitted,
        action: evaluation.outcome.action_str().to_string(),
        transformed_content: evaluation.outcome.transformed,
        findings,
    }
}

/// The most recent incidents of one sandbox, newest first.
pub async fn list_incidents(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    limit: i64,
) -> Result<Vec<PrivacyIncident>, ApiError> {
    require(principal, Permission::TelemetryRead)?;
    let limit = if limit <= 0 {
        DEFAULT_INCIDENTS
    } else {
        limit.min(MAX_INCIDENTS)
    };
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let incidents = store::list_incidents(&mut scope, sandbox_id, limit)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(incidents)
}

/// Delete incidents older than `days` and report how many were removed.
///
/// `PolicyGlobal` because this is a destructive act over the environment's
/// evidence: an operator may tune the policy, an admin decides how long it is
/// kept. `purge` is the scheduled caller of the same window.
pub async fn purge(
    ctx: &ServiceContext,
    principal: &Principal,
    days: i64,
) -> Result<u64, ApiError> {
    require(principal, Permission::PolicyGlobal)?;
    if !(1..=3650).contains(&days) {
        return Err(ApiError::validation("days must be between 1 and 3650."));
    }
    let cutoff: DateTime<Utc> = Utc::now() - chrono::Duration::days(days);
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let removed = store::purge_before(&mut scope, cutoff)
        .await
        .map_err(ApiError::from)?;
    crate::store::postgres::audit::record(
        &mut scope,
        crate::store::postgres::audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "privacy.purge",
            subject_type: Some("environment"),
            subject_id: Some(principal.environment_id),
            detail: &json!({ "days": days, "incidents": removed }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(mode: PrivacyMode) -> PrivacySettings {
        PrivacySettings {
            mode,
            ..PrivacySettings::strict_default()
        }
    }

    #[test]
    fn the_mode_matrix_does_what_each_mode_promises() {
        let content = "password=hunter2 and ada@example.com";

        let off = evaluate(&policy(PrivacyMode::Off), Direction::Outbound, Some("api"), content);
        assert_eq!(off.outcome.action_str(), "allowed");
        assert!(off.outcome.permitted);
        assert_eq!(off.outcome.transformed, content);
        assert!(off.incidents.is_empty());
        assert!(off.outcome.decisions.is_empty());

        let observe = evaluate(
            &policy(PrivacyMode::Observe),
            Direction::Outbound,
            Some("api"),
            content,
        );
        assert_eq!(observe.outcome.action_str(), "allowed");
        assert!(observe.outcome.permitted);
        assert_eq!(observe.outcome.transformed, content);
        assert_eq!(observe.incidents.len(), 2);
        for incident in &observe.incidents {
            assert_eq!(incident.action, IncidentAction::Allowed);
            assert!(incident.cloud_delivery, "observe records the pass-through");
        }

        let review = evaluate(
            &policy(PrivacyMode::Review),
            Direction::Outbound,
            Some("api"),
            content,
        );
        assert_eq!(review.outcome.action_str(), "replaced");
        assert!(review.outcome.permitted, "review never refuses");
        assert!(!review.outcome.transformed.contains("hunter2"));
        assert!(!review.outcome.transformed.contains("ada@example.com"));
        assert_eq!(review.incidents.len(), 2);
        for incident in &review.incidents {
            assert_ne!(incident.action, IncidentAction::Blocked);
            assert!(incident.cloud_delivery);
        }

        let strict = evaluate(
            &policy(PrivacyMode::Strict),
            Direction::Outbound,
            Some("api"),
            content,
        );
        assert_eq!(strict.outcome.action_str(), "blocked");
        assert!(!strict.outcome.permitted);
        assert!(!strict.outcome.transformed.contains("hunter2"));
        assert!(!strict.outcome.transformed.contains("ada@example.com"));
        assert!(
            strict
                .incidents
                .iter()
                .any(|incident| incident.action == IncidentAction::Blocked),
            "a credential blocks in strict mode"
        );
        for incident in &strict.incidents {
            assert!(!incident.cloud_delivery, "a blocked request delivered nothing");
        }
    }

    #[test]
    fn strict_without_the_egress_gate_still_redacts_but_does_not_refuse() {
        let mut settings = policy(PrivacyMode::Strict);
        settings.block_cloud_egress = false;
        let evaluation = evaluate(&settings, Direction::Outbound, Some("api"), "password=hunter2");
        assert!(evaluation.outcome.permitted);
        assert_eq!(evaluation.outcome.action_str(), "replaced");
        assert!(!evaluation.outcome.transformed.contains("hunter2"));
        assert_eq!(evaluation.incidents.len(), 1);
        assert_eq!(evaluation.incidents[0].action, IncidentAction::Replaced);
    }

    #[test]
    fn a_detector_set_that_is_empty_runs_every_detector() {
        let settings = PrivacySettings {
            detectors: Vec::new(),
            ..PrivacySettings::strict_default()
        };
        let evaluation = evaluate(&settings, Direction::Inbound, None, "ada@example.com");
        assert_eq!(evaluation.incidents.len(), 1);
    }

    #[test]
    fn only_the_configured_detectors_run() {
        let settings = PrivacySettings {
            detectors: vec![Detector::IpAddresses],
            ..PrivacySettings::strict_default()
        };
        let evaluation = evaluate(
            &settings,
            Direction::Outbound,
            None,
            "ada@example.com from 10.20.30.40",
        );
        assert_eq!(evaluation.incidents.len(), 1);
        assert_eq!(evaluation.incidents[0].detector, Detector::IpAddresses);
    }

    #[test]
    fn each_detector_finds_its_own_format() {
        let cases: [(&str, Detector, &str); 10] = [
            ("mail ada@example.com now", Detector::EmailAddresses, "ada@example.com"),
            ("host 10.20.30.40", Detector::IpAddresses, "10.20.30.40"),
            ("iban DE89370400440532013000", Detector::Iban, "DE89370400440532013000"),
            ("card 4111 1111 1111 1111", Detector::CreditCards, "4111 1111 1111 1111"),
            (
                "key sk-live-0123456789abcdef",
                Detector::ApiKeys,
                "sk-live-0123456789abcdef",
            ),
            ("password=hunter2", Detector::Credentials, "hunter2"),
            ("reach api.example.com", Detector::Hostnames, "api.example.com"),
            ("call +49 30 1234567", Detector::PhoneNumbers, "+49 30 1234567"),
            ("from Ada Lovelace", Detector::PersonNames, "Ada Lovelace"),
            ("total $1,240.00", Detector::FinancialData, "$1,240.00"),
        ];

        for (text, detector, expected) in cases {
            let findings = detect(text, &Detector::ALL);
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.detector == detector && finding.value == expected),
                "{text} should yield {expected} as {}: {findings:?}",
                detector.as_str()
            );
        }
    }

    #[test]
    fn a_checksum_keeps_the_near_misses_out() {
        let iban = detect("iban DE00370400440532013000", &[Detector::Iban]);
        assert!(iban.is_empty(), "a wrong IBAN check digit is not an IBAN");

        let card = detect("card 4111111111111112", &[Detector::CreditCards]);
        assert!(card.is_empty(), "a number that fails Luhn is not a card");

        let card = detect("card 4111 1111 1111 1111", &[Detector::CreditCards]);
        assert_eq!(card.len(), 1);
    }

    #[test]
    fn a_bare_digit_run_is_not_a_phone_number() {
        assert!(detect("order 1234567890 shipped", &[Detector::PhoneNumbers]).is_empty());
        assert_eq!(detect("call 030-1234567", &[Detector::PhoneNumbers]).len(), 1);
    }

    #[test]
    fn a_file_name_is_not_a_hostname() {
        assert!(detect("see notes.md and report.txt", &[Detector::Hostnames]).is_empty());
        assert_eq!(detect("see build.example.net now", &[Detector::Hostnames]).len(), 1);
    }

    #[test]
    fn one_value_is_reported_once() {
        let findings = detect("ada@example.com and ada@example.com", &[Detector::EmailAddresses]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].value, "ada@example.com");
    }

    #[test]
    fn a_fingerprint_is_sha256_hex_of_the_value() {
        let first = fingerprint("hunter2");
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_eq!(first, fingerprint("hunter2"));
        assert_ne!(first, fingerprint("hunter3"));
        // A known digest, so the function cannot silently change its input.
        assert_eq!(
            fingerprint("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_finding_view_never_carries_the_value() {
        let secret = "sk-live-0123456789abcdef";
        let evaluation = evaluate(
            &policy(PrivacyMode::Strict),
            Direction::Outbound,
            Some("api.provider.example"),
            &format!("key {secret}"),
        );
        let result = view(evaluation);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].detector, "api_keys");
        assert_eq!(result.findings[0].action, "blocked");
        assert!(!result.findings[0].masked_value.contains(secret));
        assert!(!result.transformed_content.contains(secret));
        assert_eq!(result.action, "blocked");
        assert!(!result.permitted);
    }

    #[test]
    fn cloud_delivery_follows_the_mode_and_the_direction() {
        assert!(!cloud_delivery(
            PrivacyMode::Strict,
            Direction::Outbound,
            Some("api"),
            true
        ));
        assert!(cloud_delivery(
            PrivacyMode::Observe,
            Direction::Inbound,
            None,
            false
        ));
        assert!(cloud_delivery(
            PrivacyMode::Review,
            Direction::Outbound,
            Some("api"),
            false
        ));
        assert!(!cloud_delivery(
            PrivacyMode::Review,
            Direction::Inbound,
            Some("api"),
            false
        ));
        assert!(!cloud_delivery(
            PrivacyMode::Strict,
            Direction::Outbound,
            None,
            false
        ));
        assert!(!cloud_delivery(
            PrivacyMode::Off,
            Direction::Outbound,
            Some("api"),
            false
        ));
    }

    #[test]
    fn a_direction_parses_its_own_spelling() {
        assert_eq!(Direction::parse("inbound"), Some(Direction::Inbound));
        assert_eq!(Direction::parse("outbound"), Some(Direction::Outbound));
        assert_eq!(Direction::parse("sideways"), None);
        assert_eq!(Direction::Outbound.as_str(), "outbound");
    }
}

