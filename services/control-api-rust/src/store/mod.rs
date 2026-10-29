//! Persistence: a tenant-scoped transaction and the repositories built on it.
//!
//! The important invariant is in `TenantScope`: it is the only way to obtain a
//! connection for tenant data, and it sets `app.environment_id` before handing
//! one out. A repository therefore cannot accidentally query without a scope,
//! because it cannot obtain a connection without one.

use uuid::Uuid;

use crate::error::StoreError;

pub mod postgres;

/// A transaction with the tenant scope set.
///
/// Row-level security reads `app.environment_id`, so setting it is what makes
/// the database enforce the same filter the application does. Two independent
/// controls, because the failure mode — returning another tenant's rows — is
/// silent and looks like success.
pub struct TenantScope<'a> {
    tx: sqlx::Transaction<'a, sqlx::Postgres>,
    environment_id: Uuid,
}

impl<'a> TenantScope<'a> {
    /// Begin a transaction and set the tenant scope.
    pub async fn begin(
        pool: &'a sqlx::PgPool,
        environment_id: Uuid,
    ) -> Result<Self, StoreError> {
        let mut tx = pool.begin().await?;
        // `set_config` with `is_local = true` is the parameterised form of
        // `SET LOCAL`, so the value cannot be injected.
        sqlx::query("SELECT set_config('app.environment_id', $1, true)")
            .bind(environment_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(Self { tx, environment_id })
    }

    /// Begin a transaction for a request: the environment, and who is acting.
    ///
    /// The acting user and role are set next to the environment, so row-level security
    /// can decide what that user may see (a user sees their own sandboxes, an admin
    /// all of them) for every query in the transaction.
    pub async fn begin_for(
        pool: &'a sqlx::PgPool,
        principal: &crate::domain::auth::Principal,
    ) -> Result<Self, StoreError> {
        let scope = Self::begin(pool, principal.environment_id).await?;
        let mut scope = scope;
        sqlx::query("SELECT set_config('app.actor_id', $1, true), set_config('app.actor_role', $2, true)")
            .bind(principal.user_id.to_string())
            .bind(principal.role.as_str())
            .execute(&mut *scope.tx)
            .await?;
        Ok(scope)
    }

    /// The environment this scope is bound to. Repositories use it as the first
    /// bind parameter, which is what keeps application and database filters
    /// identical.
    pub fn environment_id(&self) -> Uuid {
        self.environment_id
    }

    /// The underlying transaction, for a repository to run a query on.
    pub fn connection(&mut self) -> &mut sqlx::PgConnection {
        &mut self.tx
    }

    /// Commit. Writes become visible only here, which is what makes a
    /// multi-statement operation atomic.
    pub async fn commit(self) -> Result<(), StoreError> {
        self.tx.commit().await?;
        Ok(())
    }
}

/// Map a SQLx error to the store's error type, distinguishing a version
/// conflict and a unique violation from a genuine database failure.
///
/// The distinction matters because the first two are legitimate client outcomes
/// (409) and the third is a 500.
pub fn map_sqlx_error(error: sqlx::Error) -> StoreError {
    if let sqlx::Error::Database(db_error) = &error {
        if let Some(code) = db_error.code() {
            match code.as_ref() {
                // unique_violation
                "23505" => {
                    return StoreError::UniqueViolation(db_error.message().to_string());
                }
                // exclusion_violation or serialization_failure
                "23P01" | "40001" => return StoreError::VersionConflict,
                // insufficient_privilege, which for us means RLS refused.
                "42501" => return StoreError::TenantScopeViolation,
                _ => {}
            }
        }
    }
    StoreError::Database(error)
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        map_sqlx_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generic_error_stays_a_database_error() {
        let mapped = map_sqlx_error(sqlx::Error::RowNotFound);
        assert!(matches!(mapped, StoreError::Database(_)));
    }
}
