//! Reaching a sandbox from outside: SSH keys, published ports, sessions.
//!
//! The SSH key pair is generated here, written as a 0600 file under the node's
//! secret directory, and recorded as a locator (ADR 0005). The private half
//! leaves the server exactly once, through the one type built for it
//! (`domain::access::SshKeyDownload`); a second download answers 410 until the
//! key is rotated, and rotation deletes the old file before writing the new one.
//!
//! A published port is claimed from the node's pool with the unique constraint
//! doing the arbitration, and released when the forward is withdrawn. The
//! runner is told to publish `-p <node-address>:<port>:<target>`, so what the
//! Access tab shows is what `docker ps` shows.
//!
//! Three rules are worth stating because they are easy to get wrong:
//!
//! * **The fingerprint, not the key, is the identifier.** Logs, audit events and
//!   responses carry the fingerprint; only the one download carries material.
//! * **The file and the row are written in one order, and a failure is undone.**
//!   A file without a row is deleted on the next attempt; a row without a file
//!   is a broken state the operator fixes by rotating.
//! * **The port is claimed by the database.** A prescan of occupied ports only
//!   makes the search short; `ON CONFLICT DO NOTHING` is what decides.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ssh_key::{Algorithm, LineEnding, PrivateKey};
use std::io::Write;
use uuid::Uuid;

use super::{require, ServiceContext};
use crate::domain::access::{
    self, ForwardPurpose, NodePortPool, PublishedPort, SandboxSession, SshKey, SshKeyDownload,
};
use crate::domain::auth::{Permission, Principal};
use crate::domain::pack_access::{self, ForwardState, PackAccess};
use crate::domain::runtime::ForwardRequest;
use crate::domain::sandbox::Sandbox;
use crate::error::{ApiError, ErrorCode, StoreError};
use crate::security::secrets;
use crate::store::TenantScope;
use crate::store::postgres::access as store;
use crate::store::postgres::audit;
use crate::store::postgres::sandboxes as sandbox_store;

/// The container port `sshd` listens on. Every sandbox gets it published.
const SSH_TARGET_PORT: i32 = 22;

/// How many times the claim loop re-reads the occupied ports before it declares
/// the pool exhausted. Each round retries only the ports that are confirmed
/// free, so two rounds is already generous; more would spin on a busy node.
const CLAIM_ROUNDS: usize = 3;

/// The sessions a listing returns at most.
const SESSION_LIMIT: i64 = 100;

/// Where the private keys live inside the secret directory.
const SSH_SUBDIR: &str = "ssh";

/// What provisioning a sandbox produced.
pub struct Provisioned {
    pub key: SshKey,
    pub ports: Vec<PublishedPort>,
}

/// Key material, held just long enough to write one file.
///
/// Deliberately not `Serialize`, not `Clone`, and not returned: the only path
/// out of this module for the private half is `SshKeyDownload`.
struct KeyMaterial {
    public_key: String,
    private_pem: String,
    fingerprint: String,
}

/// Why writing the private key file failed.
enum KeyFileError {
    /// A file with that name is already there. The caller decides whether that
    /// is a stale leftover or a pair it should reuse.
    AlreadyExists,
    Other,
}

impl KeyFileError {
    fn to_api(self, sandbox_id: Uuid) -> ApiError {
        match self {
            Self::AlreadyExists => ApiError::internal(
                "A private key file is in the way and no key row refers to it.",
            )
            .with_details(serde_json::json!({ "sandbox_id": sandbox_id })),
            Self::Other => ApiError::internal(
                "The SSH key pair could not be written to the node's secret directory.",
            )
            .with_details(serde_json::json!({ "sandbox_id": sandbox_id })),
        }
    }
}

/// Generate an Ed25519 pair. Synchronous: it is a few hundred microseconds of
/// CPU with no I/O, and it never touches the database.
fn generate(comment: &str) -> Result<KeyMaterial, ApiError> {
    let mut rng = rand::rngs::OsRng;
    let mut key = PrivateKey::random(&mut rng, Algorithm::Ed25519).map_err(|error| {
        ApiError::internal("The SSH key pair could not be generated.")
            .with_internal(error.to_string())
    })?;
    // The comment names the sandbox, so `ssh -v` and `authorized_keys` say what
    // a key is for. It is not a secret.
    key.set_comment(comment);

    let public_key = key.public_key().to_openssh().map_err(|error| {
        ApiError::internal("The SSH key pair could not be generated.")
            .with_internal(error.to_string())
    })?;
    let private_pem = key.to_openssh(LineEnding::LF).map_err(|error| {
        ApiError::internal("The SSH key pair could not be generated.")
            .with_internal(error.to_string())
    })?;
    let private_pem = private_pem.to_string();
    let fingerprint = key
        .public_key()
        .fingerprint(ssh_key::HashAlg::Sha256)
        .to_string();

    Ok(KeyMaterial {
        public_key,
        private_pem,
        fingerprint,
    })
}

/// The file a sandbox's private key lives in.
fn key_path(secret_dir: &Path, sandbox_id: Uuid) -> PathBuf {
    secret_dir.join(SSH_SUBDIR).join(format!("{sandbox_id}.key"))
}

/// The locator stored in the database. A locator, never material (ADR 0005).
fn locator(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Write the private key as a 0600 file under the secret directory.
///
/// `create_new` is what makes two concurrent provisions harmless: the second
/// writer fails instead of truncating the first writer's key. The mode is set
/// at creation so the file is never briefly world-readable, and set again
/// afterwards so a file that somehow existed with wider permissions is fixed.
///
/// Synchronous: a few hundred bytes next to a key generation that is already
/// CPU-bound, and `security::secrets` reads the same files the same way.
fn write_key_file(
    secret_dir: &Path,
    sandbox_id: Uuid,
    pem: &str,
) -> Result<PathBuf, KeyFileError> {
    let dir = secret_dir.join(SSH_SUBDIR);
    std::fs::create_dir_all(&dir).map_err(|_| KeyFileError::Other)?;
    set_mode(&dir, 0o700).map_err(|_| KeyFileError::Other)?;

    let path = key_path(secret_dir, sandbox_id);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(KeyFileError::AlreadyExists);
        }
        Err(_) => return Err(KeyFileError::Other),
    };
    if file.write_all(pem.as_bytes()).is_err() || file.flush().is_err() {
        // A half-written key file is worse than none: it would be served as if
        // it were a key. Remove it.
        let _ = std::fs::remove_file(&path);
        return Err(KeyFileError::Other);
    }
    drop(file);
    if set_mode(&path, 0o600).is_err() {
        let _ = std::fs::remove_file(&path);
        return Err(KeyFileError::Other);
    }
    Ok(path)
}

/// Set a path's mode. Separate so the `cfg(unix)` is in one place.
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Remove a key file, ignoring "it was already gone".
fn remove_key_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            // The path is a secret path, the error is not a secret. Log it so a
            // rotation that cannot delete is visible rather than silent.
            tracing::warn!(error = %error, "could not remove an SSH key file");
        }
    }
}

/// Write the pair, replacing a stale file from a crashed earlier attempt.
fn write_key_file_fresh(
    secret_dir: &Path,
    sandbox_id: Uuid,
    pem: &str,
) -> Result<PathBuf, ApiError> {
    match write_key_file(secret_dir, sandbox_id, pem) {
        Ok(path) => Ok(path),
        Err(KeyFileError::AlreadyExists) => {
            // No key row refers to this file (the caller checked), so it is a
            // leftover. Deleting it is safe: without a row it was never
            // downloadable.
            remove_key_file(&key_path(secret_dir, sandbox_id));
            write_key_file(secret_dir, sandbox_id, pem).map_err(|error| error.to_api(sandbox_id))
        }
        Err(error) => Err(error.to_api(sandbox_id)),
    }
}

fn map_sandbox_missing(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::SANDBOX_NOT_FOUND,
            "No such sandbox in this environment.",
        ),
        other => ApiError::from(other),
    }
}

fn map_key_missing(error: StoreError) -> ApiError {
    match error {
        StoreError::NotFound => ApiError::not_found(
            ErrorCode::SSH_KEY_NOT_FOUND,
            "This sandbox has no SSH key pair yet.",
        ),
        other => ApiError::from(other),
    }
}

/// Look a sandbox up inside the caller's tenant, so a foreign id is a 404.
async fn load_sandbox(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Sandbox, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sandbox = sandbox_store::get(&mut scope, sandbox_id)
        .await
        .map_err(map_sandbox_missing)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(sandbox)
}

/// The stored key pair, never the material.
pub async fn key(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<SshKey, ApiError> {
    require(principal, Permission::SandboxRead)?;
    load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let key = store::get_key(&mut scope, sandbox_id)
        .await
        .map_err(map_key_missing)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(key)
}

/// The one-time private key download.
///
/// The material is read before the download is stamped, so an unreadable file
/// does not burn the operator's single chance. The stamp itself is a conditional
/// `UPDATE`: exactly one caller wins, and the loser gets 410.
pub async fn download(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<SshKeyDownload, ApiError> {
    // A key is a shell. Reading one out is the same privilege as running a
    // command inside the sandbox, so that is the permission it needs.
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let key = store::get_key(&mut scope, sandbox_id)
        .await
        .map_err(map_key_missing)?;
    scope.commit().await.map_err(ApiError::from)?;

    let material = secrets::resolve_verbatim(&key.private_key_ref, &ctx.secret_dir).map_err(
        |error| {
            // The message names the problem without naming the path or the key.
            ApiError::unprocessable(ErrorCode::SSH_KEY_UNREADABLE, error.message())
        },
    )?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let won = store::stamp_download(&mut scope, sandbox_id, principal.user_id)
        .await
        .map_err(ApiError::from)?;
    if !won {
        return Err(ApiError::gone(
            ErrorCode::SSH_KEY_ALREADY_DOWNLOADED,
            "This key was already downloaded. Rotate it to get a new one.",
        ));
    }
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_key_downloaded",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            // The fingerprint identifies the key without being the key.
            detail: &serde_json::json!({
                "fingerprint": key.fingerprint,
                "public_key": key.public_key,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    Ok(SshKeyDownload {
        sandbox_id,
        fingerprint: key.fingerprint,
        private_key_pem: material,
        filename: format!("nestbox-{}.key", sandbox.name),
    })
}

/// Replace the key pair: delete the old file, write the new one, reset the
/// download mark.
pub async fn rotate(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<SshKey, ApiError> {
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;

    // Deleting first matters: a crash between the two steps must not leave two
    // usable keys (ADR 0024).
    remove_key_file(&key_path(&ctx.secret_dir, sandbox_id));

    let material = generate(&format!("nestbox:{}", sandbox.name))?;
    let path = write_key_file_fresh(&ctx.secret_dir, sandbox_id, &material.private_pem)?;
    let reference = locator(&path);

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let stored = match store::replace_key(
        &mut scope,
        sandbox_id,
        &material.public_key,
        &reference,
        &material.fingerprint,
    )
    .await
    {
        Ok(key) => key,
        Err(StoreError::NotFound) => store::insert_key(
            &mut scope,
            store::NewSshKey {
                sandbox_id,
                public_key: &material.public_key,
                private_key_ref: &reference,
                fingerprint: &material.fingerprint,
            },
        )
        .await
        .map_err(ApiError::from)?,
        Err(other) => {
            // The row still describes the old key while the file on disk is the
            // new one. Remove the new file so the state is "no usable key",
            // which the next rotation fixes, instead of a key that does not
            // match its fingerprint.
            remove_key_file(&path);
            return Err(ApiError::from(other));
        }
    };

    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_key_rotated",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({ "fingerprint": stored.fingerprint }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    // The new public key must reach the runner, or the old one keeps working
    // and the new one is refused until something else pushes the set.
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(stored)
}

/// Create the key pair and the published ports for a sandbox that was just
/// created.
///
/// Called once, from the create path, so a sandbox never exists without a way
/// in. It is idempotent: an existing key is kept, and existing forwards are
/// returned instead of claiming more ports.
pub async fn provision(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: &Sandbox,
) -> Result<Provisioned, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let key = ensure_key(ctx, principal, sandbox).await?;
    let ports = claim_sandbox_ports(ctx, principal, sandbox).await?;
    // Reserving a port is bookkeeping; the runner is what opens it. Done here so
    // a sandbox is reachable the moment it exists, with no second step.
    push_forwards_logged(ctx, principal, sandbox).await;
    Ok(Provisioned { key, ports })
}

/// Hand the runner the sandbox's complete set of ports and its public key, and
/// record what it opened in the `forwards` rows.
///
/// Called whenever the set or the key changes (create, publish, withdraw,
/// rotate) and when a read finds the runner out of step with the database. It
/// is the same call each time, so none of those paths needs its own recovery.
/// No permission check: it only pushes what the database already says, and the
/// callers have checked what they need to.
pub async fn push_forwards(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: &Sandbox,
) -> Result<Vec<ForwardState>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let ports = store::list_forwards(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    let public_key = match store::get_key(&mut scope, sandbox.id).await {
        Ok(key) => Some(key.public_key),
        Err(StoreError::NotFound) => None,
        Err(error) => return Err(ApiError::from(error)),
    };
    let user_keys = store::list_user_keys(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    let credential_rows = store::list_credentials(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;

    // Only logins that are still good are sent; the runner checks the expiry again
    // on every attempt, so a clock skew here cannot extend one.
    let credentials: Vec<crate::domain::runtime::SshCredential> = credential_rows
        .iter()
        .filter(|row| row.credential.state == "active")
        .map(|row| crate::domain::runtime::SshCredential {
            username: row.credential.username.clone(),
            salt: row.salt.clone(),
            hash: row.hash.clone(),
            expires_at: row.credential.expires_at.timestamp(),
        })
        .collect();

    let requests: Vec<ForwardRequest> = ports
        .iter()
        .map(|port| ForwardRequest {
            port: port.public_port,
            target_port: port.target_port,
            purpose: port.purpose.as_str().to_string(),
        })
        .collect();
    let keys: Vec<String> = public_key
        .into_iter()
        .chain(user_keys.into_iter().map(|key| key.public_key))
        .collect();

    let states = ctx
        .runtime
        .set_forwards(&sandbox.name, &requests, &keys, &credentials)
        .await
        .map_err(|error| {
            ApiError::internal("The runner did not open the published ports.")
                .with_internal(error.to_string())
        })?;

    let updates: Vec<(i32, &str)> = ports
        .iter()
        .map(|port| {
            let open = states
                .iter()
                .any(|state| state.port == port.public_port && state.listening);
            (port.public_port, if open { "active" } else { "failed" })
        })
        .collect();
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::set_forward_states(&mut scope, sandbox.id, &updates)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(states)
}

/// `push_forwards` for a path that must not fail because the runner did: the
/// forwards stay `pending` and the next read of the Access panel pushes again.
pub async fn push_forwards_logged(ctx: &ServiceContext, principal: &Principal, sandbox: &Sandbox) {
    if let Err(error) = push_forwards(ctx, principal, sandbox).await {
        tracing::warn!(
            sandbox = %sandbox.name,
            code = error.code.0,
            "published ports could not be pushed to the runner; they stay pending"
        );
    }
}

/// What the runner reports for a sandbox's ports, repaired when it is behind.
///
/// A runner that was recreated without its state volume has no listeners; the
/// database still has every forward. Pushing them again on read means an
/// operator never has to know the runner restarted.
async fn live_forward_states(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: &Sandbox,
    ports: &[PublishedPort],
) -> Option<Vec<ForwardState>> {
    if ports.is_empty() {
        return Some(Vec::new());
    }
    let asked = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        ctx.runtime.forward_states(&sandbox.name),
    )
    .await;
    let states = match asked {
        Ok(Ok(states)) => states,
        _ => return None,
    };
    let complete = ports
        .iter()
        .all(|port| states.iter().any(|state| state.port == port.public_port));
    if complete {
        return Some(states);
    }
    push_forwards(ctx, principal, sandbox).await.ok()
}

/// The key pair of a sandbox, created on first use.
async fn ensure_key(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: &Sandbox,
) -> Result<SshKey, ApiError> {
    let existing = {
        let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
            .await
            .map_err(ApiError::from)?;
        let found = match store::get_key(&mut scope, sandbox.id).await {
            Ok(key) => Some(key),
            Err(StoreError::NotFound) => None,
            Err(error) => return Err(ApiError::from(error)),
        };
        scope.commit().await.map_err(ApiError::from)?;
        found
    };
    if let Some(key) = existing {
        return Ok(key);
    }

    let material = generate(&format!("nestbox:{}", sandbox.name))?;
    let path = write_key_file_fresh(&ctx.secret_dir, sandbox.id, &material.private_pem)?;
    let reference = locator(&path);

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let key = match store::insert_key(
        &mut scope,
        store::NewSshKey {
            sandbox_id: sandbox.id,
            public_key: &material.public_key,
            private_key_ref: &reference,
            fingerprint: &material.fingerprint,
        },
    )
    .await
    {
        Ok(key) => key,
        Err(error) => {
            // Keep the disk and the row consistent: no row, no file.
            remove_key_file(&path);
            return Err(ApiError::from(error));
        }
    };
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_key_created",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox.id),
            detail: &serde_json::json!({
                "fingerprint": key.fingerprint,
                "public_key": key.public_key,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(key)
}

/// Claim the SSH port and one port per pack port.
async fn claim_sandbox_ports(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox: &Sandbox,
) -> Result<Vec<PublishedPort>, ApiError> {
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    // A retried create must not claim a second set of ports.
    let existing = store::list_forwards(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    if !existing.is_empty() {
        scope.commit().await.map_err(ApiError::from)?;
        return Ok(existing);
    }

    let pool = store::port_pool(&mut scope, sandbox.node_id)
        .await
        .map_err(ApiError::from)?;

    let mut wanted: Vec<(ForwardPurpose, i32, String)> = vec![(
        ForwardPurpose::Ssh,
        SSH_TARGET_PORT,
        "ssh".to_string(),
    )];
    let pack_ports = store::pack_ports(&mut scope, &sandbox.pack_key)
        .await
        .map_err(ApiError::from)?;
    for port in pack_ports {
        // Port 22 is already published for every sandbox; a pack that lists it
        // would otherwise try to claim a second forward for the same listener.
        if port == SSH_TARGET_PORT {
            continue;
        }
        wanted.push((ForwardPurpose::Agent, port, sandbox.pack_key.clone()));
    }

    let mut claimed = Vec::with_capacity(wanted.len());
    for (purpose, target_port, label) in wanted {
        let port = claim_port(&mut scope, &pool, sandbox.id, purpose, target_port).await?;
        let forward_id = store::insert_forward(
            &mut scope,
            store::NewForward {
                sandbox_id: sandbox.id,
                local_port: port,
                bind_address: pool.public_address.as_deref().unwrap_or("127.0.0.1"),
                target_port,
                purpose,
                label: Some(&label),
            },
        )
        .await
        .map_err(ApiError::from)?;
        store::link_forward(&mut scope, pool.node_id, port, sandbox.id, forward_id)
            .await
            .map_err(ApiError::from)?;

        claimed.push(PublishedPort {
            forward_id,
            sandbox_id: sandbox.id,
            node_id: pool.node_id,
            public_address: pool.public_address.clone(),
            public_port: port,
            target_port,
            purpose,
            label: Some(label),
            state: "pending".to_string(),
        });
    }

    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ports_published",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox.id),
            detail: &serde_json::json!({
                "ports": claimed
                    .iter()
                    .map(|port| serde_json::json!({
                        "forward_id": port.forward_id,
                        "public_port": port.public_port,
                        "target_port": port.target_port,
                        "purpose": port.purpose.as_str(),
                    }))
                    .collect::<Vec<_>>(),
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(claimed)
}

/// Claim one free port from the node's range.
///
/// The prescan shortens the search; the insert decides. Candidates start at a
/// sandbox-dependent offset so parallel creates do not all probe the bottom of
/// the range first.
async fn claim_port(
    scope: &mut TenantScope<'_>,
    pool: &NodePortPool,
    sandbox_id: Uuid,
    purpose: ForwardPurpose,
    target_port: i32,
) -> Result<i32, ApiError> {
    for _ in 0..CLAIM_ROUNDS {
        let taken: HashSet<i32> = store::claimed_ports(scope, pool.node_id)
            .await
            .map_err(ApiError::from)?
            .into_iter()
            .collect();
        let mut candidates: Vec<i32> = (pool.start..=pool.end)
            .filter(|port| !taken.contains(port))
            .collect();
        if candidates.is_empty() {
            break;
        }
        let offset = offset_for(sandbox_id, candidates.len());
        candidates.rotate_left(offset);

        for port in candidates {
            let won = store::claim_port(scope, pool.node_id, port, sandbox_id, purpose, target_port)
                .await
                .map_err(ApiError::from)?;
            if won {
                return Ok(port);
            }
        }
    }

    Err(ApiError::conflict(
        ErrorCode::PORT_POOL_EXHAUSTED,
        format!(
            "Node has no free port in {}-{}.",
            pool.start, pool.end
        ),
    ))
}

/// A candidate index derived from the sandbox id.
///
/// Deterministic (a retry probes the same order) and spread over the range, so
/// two sandboxes created at the same moment rarely start on the same port.
fn offset_for(sandbox_id: Uuid, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let bytes = sandbox_id.as_bytes();
    let mut seed: u64 = 0;
    for byte in &bytes[..8] {
        seed = seed.wrapping_mul(31).wrapping_add(u64::from(*byte));
    }
    (seed % len as u64) as usize
}

/// Every port published for a sandbox.
pub async fn ports(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<PublishedPort>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let ports = store::list_forwards(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(ports)
}

/// Everything needed to reach one sandbox from outside: the node's address and
/// hostname, every published port, and the SSH key's state.
///
/// One read instead of three on the client, so the panel cannot show ports from
/// one moment and an address from another.
pub async fn connect(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<crate::domain::access::ConnectInfo, ApiError> {
    require(principal, Permission::SandboxRead)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;

    // A sandbox created before ports were published has no key and no forwards.
    // "A sandbox never exists without a way in" is a promise of `provision`, so a
    // reader who is allowed to write gives it one here, once; a viewer only sees
    // that nothing is published yet. Best effort: the panel must still render.
    if require(principal, Permission::SandboxWrite).is_ok() {
        let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
            .await
            .map_err(ApiError::from)?;
        let none_yet = store::list_forwards(&mut scope, sandbox.id)
            .await
            .map_err(ApiError::from)?
            .is_empty();
        scope.commit().await.map_err(ApiError::from)?;
        if none_yet {
            if let Err(error) = provision(ctx, principal, &sandbox).await {
                tracing::warn!(sandbox = %sandbox.name, code = error.code.0, "late provisioning failed");
            }
        }
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let endpoint = store::node_endpoint(&mut scope, sandbox.node_id)
        .await
        .map_err(ApiError::from)?;
    let ports = store::list_forwards(&mut scope, sandbox.id)
        .await
        .map_err(ApiError::from)?;
    let key = match store::get_key(&mut scope, sandbox.id).await {
        Ok(key) => Some(crate::domain::access::ConnectKey {
            downloadable: key.downloadable(),
            fingerprint: key.fingerprint,
        }),
        Err(StoreError::NotFound) => None,
        Err(error) => return Err(ApiError::from(error)),
    };
    let pack = PackAccess::from_value(
        &store::pack_access(&mut scope, &sandbox.pack_key)
            .await
            .map_err(ApiError::from)?,
    );
    scope.commit().await.map_err(ApiError::from)?;

    let live = live_forward_states(ctx, principal, &sandbox, &ports).await;

    let info = crate::domain::access::build_connect(
        crate::domain::access::ConnectInput {
            sandbox_id: sandbox.id,
            sandbox_name: &sandbox.name,
            node_id: sandbox.node_id,
            node_name: &endpoint.name,
            address: endpoint.address.as_deref(),
            hostname: endpoint.hostname.as_deref(),
            ports: &ports,
            key,
        },
    );
    Ok(pack_access::enrich(info, &sandbox.pack_key, &pack, live.as_deref()))
}

/// Set the address and hostname clients use for a node.
///
/// The address is where ports are bound, so it must be an IP literal; the
/// hostname is a label and must look like a DNS name. Ports already published
/// keep the binding they were made with: changing this does not move them.
pub async fn set_node_endpoint(
    ctx: &ServiceContext,
    principal: &Principal,
    node_id: Uuid,
    address: Option<&str>,
    hostname: Option<&str>,
) -> Result<(), ApiError> {
    require(principal, Permission::NodeManage)?;
    let address = address.map(str::trim).filter(|text| !text.is_empty());
    let hostname = hostname
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_ascii_lowercase);
    if let Some(address) = address {
        if address.parse::<std::net::IpAddr>().is_err() {
            return Err(ApiError::validation(
                "The address is an IP address such as 192.168.1.20.",
            ));
        }
    }
    if let Some(name) = hostname.as_deref() {
        let label_ok = |label: &str| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        };
        if name.len() > 253 || !name.split('.').all(label_ok) {
            return Err(ApiError::validation(
                "The hostname is a DNS name such as box.example.net.",
            ));
        }
    }

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::set_node_endpoint(&mut scope, node_id, address, hostname.as_deref())
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(())
}

/// Publish one more container port from the node's pool.
pub async fn publish_port(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    target_port: i32,
    label: Option<String>,
) -> Result<PublishedPort, ApiError> {
    require(principal, Permission::SandboxWrite)?;
    if !access::is_publishable_port(target_port) {
        return Err(ApiError::validation(
            "The target port must be between 1 and 65535.",
        ));
    }
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;

    let existing = store::list_forwards(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?;
    if let Some(port) = existing
        .iter()
        .find(|port| port.target_port == target_port)
    {
        // One forward per container port: a second would compete for the same
        // listener and make "which URL is mine" ambiguous.
        return Err(ApiError::conflict(
            ErrorCode::CONFLICT,
            format!(
                "Port {target_port} is already published on {}.",
                port.public_port
            ),
        ));
    }

    let pool = store::port_pool(&mut scope, sandbox.node_id)
        .await
        .map_err(ApiError::from)?;
    let purpose = ForwardPurpose::Custom;
    let port = claim_port(&mut scope, &pool, sandbox.id, purpose, target_port).await?;
    let forward_id = store::insert_forward(
        &mut scope,
        store::NewForward {
            sandbox_id,
            local_port: port,
            bind_address: pool.public_address.as_deref().unwrap_or("127.0.0.1"),
            target_port,
            purpose,
            label: label.as_deref(),
        },
    )
    .await
    .map_err(ApiError::from)?;
    store::link_forward(&mut scope, pool.node_id, port, sandbox_id, forward_id)
        .await
        .map_err(ApiError::from)?;

    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.port_published",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({
                "forward_id": forward_id,
                "public_port": port,
                "target_port": target_port,
            }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;

    Ok(PublishedPort {
        forward_id,
        sandbox_id,
        node_id: pool.node_id,
        public_address: pool.public_address,
        public_port: port,
        target_port,
        purpose,
        label,
        state: "pending".to_string(),
    })
}

/// Withdraw a published port and give it back to the pool.
pub async fn withdraw_port(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    forward_id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::SandboxWrite)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let released = store::delete_forward(&mut scope, sandbox_id, forward_id).await;
    match released {
        Ok(()) => {}
        Err(StoreError::NotFound) => {
            return Err(ApiError::not_found(
                ErrorCode::FORWARD_NOT_FOUND,
                "No such published port on this sandbox.",
            ));
        }
        Err(error) => return Err(ApiError::from(error)),
    }
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.port_withdrawn",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({ "forward_id": forward_id }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(())
}

/// Terminal and SSH sessions of one sandbox: who, when, how long, how many
/// bytes. Never the content.
pub async fn sessions(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<Vec<SandboxSession>, ApiError> {
    require(principal, Permission::SandboxRead)?;
    load_sandbox(ctx, principal, sandbox_id).await?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let sessions = store::list_sessions(&mut scope, sandbox_id, SESSION_LIMIT)
        .await
        .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_pair_is_a_matching_ed25519_pair() {
        let material = generate("nestbox:unit").expect("generates");
        assert!(material.public_key.starts_with("ssh-ed25519 "));
        assert!(material.public_key.contains("nestbox:unit"));
        assert!(material.private_pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(material.private_pem.trim_end().ends_with("-----END OPENSSH PRIVATE KEY-----"));
        assert!(material.fingerprint.starts_with("SHA256:"));
        // The public half inside the private file is the half we advertise.
        let parsed = PrivateKey::from_openssh(&material.private_pem).expect("parses");
        assert_eq!(
            parsed.public_key().to_openssh().expect("public"),
            material.public_key
        );
    }

    #[test]
    fn two_generated_pairs_differ() {
        let first = generate("a").expect("generates");
        let second = generate("a").expect("generates");
        assert_ne!(first.public_key, second.public_key);
        assert_ne!(first.fingerprint, second.fingerprint);
    }

    #[test]
    fn the_locator_names_the_sandbox_and_keeps_the_file_inside_the_directory() {
        let id = Uuid::nil();
        let path = key_path(Path::new("/secrets"), id);
        assert_eq!(path, PathBuf::from("/secrets/ssh/00000000-0000-0000-0000-000000000000.key"));
        assert_eq!(
            locator(&path),
            "file:///secrets/ssh/00000000-0000-0000-0000-000000000000.key"
        );
    }

    #[test]
    fn the_candidate_offset_is_stable_and_inside_the_range() {
        let id = Uuid::parse_str("6f9619ff-8b86-d011-b42d-00c04fc964ff").expect("uuid");
        let first = offset_for(id, 1000);
        assert_eq!(first, offset_for(id, 1000));
        assert!(first < 1000);
        assert_eq!(offset_for(id, 0), 0);
        assert_eq!(offset_for(id, 1), 0);
    }

    #[test]
    fn the_key_file_is_created_private_and_a_second_write_refuses_to_overwrite() {
        let dir = std::env::temp_dir().join(format!("nestbox-access-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = Uuid::nil();

        let path = write_key_file(&dir, id, "material-one").ok().expect("writes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the private key must be 0600");
        }
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "material-one");

        // A second write must not truncate the first writer's key.
        assert!(matches!(
            write_key_file(&dir, id, "material-two"),
            Err(KeyFileError::AlreadyExists)
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "material-one");

        // The recovery path replaces a stale file with a fresh one.
        let replaced = write_key_file_fresh(&dir, id, "material-three").expect("replaces");
        assert_eq!(std::fs::read_to_string(&replaced).expect("read"), "material-three");

        let _ = std::fs::remove_dir_all(&dir);
    }
}


// ---------------------------------------------------------------------------
// Operator keys and temporary logins
// ---------------------------------------------------------------------------

/// What may open this sandbox's SSH door besides the generated key.
#[derive(Debug, serde::Serialize)]
pub struct SshAccessView {
    pub generated_key_fingerprint: Option<String>,
    pub keys: Vec<crate::domain::ssh_access::UserKey>,
    pub credentials: Vec<crate::domain::ssh_access::TempCredential>,
}

pub async fn ssh_access(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
) -> Result<SshAccessView, ApiError> {
    require(principal, Permission::SandboxRead)?;
    load_sandbox(ctx, principal, sandbox_id).await?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let generated = match store::get_key(&mut scope, sandbox_id).await {
        Ok(key) => Some(key.fingerprint),
        Err(StoreError::NotFound) => None,
        Err(error) => return Err(ApiError::from(error)),
    };
    let keys = store::list_user_keys(&mut scope, sandbox_id).await.map_err(ApiError::from)?;
    let credentials = store::list_credentials(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
        .into_iter()
        .map(|row| row.credential)
        .collect();
    scope.commit().await.map_err(ApiError::from)?;
    Ok(SshAccessView { generated_key_fingerprint: generated, keys, credentials })
}

pub async fn add_user_key(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    label: &str,
    public_key: &str,
) -> Result<crate::domain::ssh_access::UserKey, ApiError> {
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    let label = label.trim();
    if label.is_empty() || label.len() > 80 {
        return Err(ApiError::validation("Give the key a label of 1 to 80 characters, e.g. the machine it belongs to."));
    }
    let (normalised, fingerprint) =
        crate::domain::ssh_access::parse_public_key(public_key).map_err(ApiError::validation)?;

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let existing = store::list_user_keys(&mut scope, sandbox_id).await.map_err(ApiError::from)?;
    if existing.len() as i64 >= crate::domain::ssh_access::MAX_USER_KEYS_PER_SANDBOX {
        return Err(ApiError::conflict(ErrorCode::CONFLICT, "This sandbox already has the most keys it may have; remove one first."));
    }
    let Some(id) = store::insert_user_key(&mut scope, sandbox_id, label, &normalised, &fingerprint, principal.user_id)
        .await
        .map_err(ApiError::from)?
    else {
        return Err(ApiError::conflict(ErrorCode::CONFLICT, "That key is already added to this sandbox."));
    };
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_user_key_added",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({ "fingerprint": fingerprint, "label": label }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(crate::domain::ssh_access::UserKey {
        id,
        label: label.to_string(),
        public_key: normalised,
        fingerprint,
        created_at: chrono::Utc::now(),
    })
}

pub async fn remove_user_key(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    key_id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::delete_user_key(&mut scope, sandbox_id, key_id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::FORWARD_NOT_FOUND, "No such key on this sandbox."),
        other => ApiError::from(other),
    })?;
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_user_key_removed",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({ "key_id": key_id }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(())
}

fn random_token(length: usize) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rngs::OsRng;
    (0..length).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect()
}

/// Make a temporary user name and password for this sandbox. The password is in
/// the answer once and nowhere else: only a salted hash is stored.
pub async fn issue_credential(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    minutes: Option<i64>,
) -> Result<crate::domain::ssh_access::IssuedCredential, ApiError> {
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    let now = chrono::Utc::now();
    let expires_at = crate::domain::ssh_access::expiry(now, crate::domain::ssh_access::clamp_minutes(minutes));

    let username = crate::domain::ssh_access::username_from(rand::RngCore::next_u32(&mut rand::rngs::OsRng));
    let password = random_token(20);
    let salt = random_token(16);
    let hash = crate::domain::ssh_access::hash_password(&salt, &password);

    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    let active = store::list_credentials(&mut scope, sandbox_id)
        .await
        .map_err(ApiError::from)?
        .iter()
        .filter(|row| row.credential.state == "active")
        .count() as i64;
    if active >= crate::domain::ssh_access::MAX_ACTIVE_CREDENTIALS_PER_SANDBOX {
        return Err(ApiError::conflict(ErrorCode::CONFLICT, "This sandbox already has the most temporary logins it may have; revoke one first."));
    }
    let id = store::insert_credential(&mut scope, sandbox_id, &username, &salt, &hash, expires_at, principal.user_id)
        .await
        .map_err(ApiError::from)?;
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_credential_issued",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            // The user name and expiry are evidence; the password is not recorded.
            detail: &serde_json::json!({ "username": username, "expires_at": expires_at }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(crate::domain::ssh_access::IssuedCredential { id, username, password, expires_at })
}

pub async fn revoke_credential(
    ctx: &ServiceContext,
    principal: &Principal,
    sandbox_id: Uuid,
    credential_id: Uuid,
) -> Result<(), ApiError> {
    require(principal, Permission::SandboxExec)?;
    let sandbox = load_sandbox(ctx, principal, sandbox_id).await?;
    let mut scope = TenantScope::begin(&ctx.pool, principal.environment_id)
        .await
        .map_err(ApiError::from)?;
    store::revoke_credential(&mut scope, sandbox_id, credential_id).await.map_err(|error| match error {
        StoreError::NotFound => ApiError::not_found(ErrorCode::FORWARD_NOT_FOUND, "No active login with that id on this sandbox."),
        other => ApiError::from(other),
    })?;
    audit::record(
        &mut scope,
        audit::NewAuditEvent {
            actor_id: Some(principal.user_id),
            actor_label: &principal.display_name,
            kind: "sandbox.ssh_credential_revoked",
            subject_type: Some("sandbox"),
            subject_id: Some(sandbox_id),
            detail: &serde_json::json!({ "credential_id": credential_id }),
            request_id: None,
        },
    )
    .await
    .map_err(ApiError::from)?;
    scope.commit().await.map_err(ApiError::from)?;
    push_forwards_logged(ctx, principal, &sandbox).await;
    Ok(())
}
