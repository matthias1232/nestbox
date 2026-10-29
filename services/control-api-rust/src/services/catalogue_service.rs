//! The live package catalogue.
//!
//! The builder offers what a distribution ships, not what a seed migration
//! remembered. The question is asked inside a container of the baseline image
//! (see `runtime::RuntimeAdapter::catalogue`), and this module only decides who
//! may ask, which baseline the question is about, and that the fragment is a
//! package name rather than shell text.
//!
//! No cache here: the runner keeps a ten-minute read-through cache next to the
//! index it talked to, which is the only place that knows whether an answer is
//! still fresh. A second cache would be a second staleness to reason about.

use super::{require, ServiceContext};
use crate::domain::auth::{Permission, Principal};
use crate::domain::runtime::{CatalogueAnswer, CatalogueRequest};
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::store::TenantScope;
use crate::store::postgres::agent_packs as store;

/// The longest fragment a caller may search for.
const MAX_QUERY: usize = 80;

/// Whether a fragment is a package-name fragment and not a shell expression.
fn valid_fragment(query: &str) -> bool {
    !query.is_empty()
        && query.len() <= MAX_QUERY
        && query
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+._:@/-".contains(c))
}

/// Search the native package index of one baseline's distribution.
///
/// An unknown baseline is a 404, an unsafe fragment a 422, and a runtime that
/// cannot answer is reported as unavailable rather than as an empty catalogue:
/// "nothing found" and "could not ask" lead an operator to opposite conclusions.
pub async fn search(
    ctx: &ServiceContext,
    principal: &Principal,
    baseline_key: &str,
    query: &str,
) -> Result<CatalogueAnswer, ApiError> {
    require(principal, Permission::RecipeRead)?;
    let query = query.trim();
    if !valid_fragment(query) {
        return Err(ApiError::validation(
            "A package search is 1 to 80 characters of letters, digits, and . + _ : @ / -.",
        ));
    }
    let mut scope = TenantScope::begin_for(&ctx.pool, principal)
        .await
        .map_err(ApiError::from)?;
    let baseline = store::get_baseline(&mut scope, baseline_key)
        .await
        .map_err(|error| match error {
            StoreError::NotFound => ApiError::not_found(
                ErrorCode::BASELINE_NOT_FOUND,
                format!("No baseline '{baseline_key}' exists in this environment."),
            ),
            other => ApiError::from(other),
        })?;
    scope.commit().await.map_err(ApiError::from)?;

    let request = CatalogueRequest {
        image: baseline.base_image,
        package_manager: baseline.package_manager.as_str().to_string(),
        query: query.to_string(),
    };
    ctx.runtime.catalogue(request).await.map_err(|error| {
        ApiError::unavailable(
            ErrorCode::NODE_RUNTIME_UNAVAILABLE,
            format!("The package index could not be queried: {error}"),
        )
    })
}
