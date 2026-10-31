//! The tenant tree, the member role, sandbox grants, invitations and sign-in through an
//! identity provider, against a real PostgreSQL as the application role.
//!
//!   TEST_DATABASE_URL=postgres://… cargo test --test multitenant -- --test-threads=1

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use uuid::Uuid;

use nestbox_control_api::config::{Config, DeployMode, OidcProviderConfig};
use nestbox_control_api::domain::auth::{CredentialSource, Principal, Role};
use nestbox_control_api::domain::pagination::ListParams;
use nestbox_control_api::events::bus::LocalBus;
use nestbox_control_api::runtime::fake::FakeRuntime;
use nestbox_control_api::services::oidc_service::{self, Profile};
use nestbox_control_api::services::sandbox_service::{self, CreateSandbox};
use nestbox_control_api::services::{auth_service, tenancy_service, user_service, ServiceContext};

const ENVIRONMENT: &str = "00000000-0000-0000-0000-000000000002";

fn uuid(value: &str) -> Uuid {
    Uuid::from_str(value).unwrap_or_else(|_| Uuid::nil())
}

async fn connect() -> Option<sqlx::PgPool> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let _ = sqlx::query("DROP SCHEMA IF EXISTS public CASCADE").execute(&pool).await;
    let _ = sqlx::query("CREATE SCHEMA public").execute(&pool).await;
    let _ = sqlx::query(
        "DO $$BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'nestbox_app') THEN
                CREATE ROLE nestbox_app LOGIN;
            END IF;
        END$$",
    )
    .execute(&pool)
    .await;
    let _ = sqlx::query("GRANT USAGE, CREATE ON SCHEMA public TO nestbox_app").execute(&pool).await;
    sqlx::migrate!("./migrations").run(&pool).await.ok()?;
    Some(pool)
}

async fn app_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("url");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE nestbox_app").execute(connection).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("app pool")
}

fn context(pool: &sqlx::PgPool) -> ServiceContext {
    ServiceContext::new(
        pool.clone(),
        Arc::new(FakeRuntime::new()),
        Arc::new(LocalBus::new()),
        PathBuf::from(std::env::temp_dir()),
    )
}

fn config(mode: DeployMode) -> Config {
    Config {
        database_url: "postgres://unused".into(),
        listen_addr: "127.0.0.1:0".into(),
        secret_dir: std::env::temp_dir(),
        session_cookie_secure: false,
        session_idle_seconds: 3600,
        session_max_seconds: 7200,
        allowed_origins: Vec::new(),
        runner_url: "http://sandbox-runner:9000".into(),
        runtime_network: "nestbox-runtime".into(),
        bootstrap_admin: false,
        body_limit_bytes: 1_048_576,
        rate_limit_per_minute: 600,
        terminal_idle_seconds: 1800,
        deploy_mode: mode,
        public_url: "http://nestbox.test".into(),
        oidc: vec![OidcProviderConfig {
            id: "google".into(),
            label: "Google".into(),
            kind: "oidc".into(),
            issuer: "http://127.0.0.1:1".into(),
            client_id: "client".into(),
            client_secret: "secret".into(),
        }],
    }
}

async fn admin_user(pool: &sqlx::PgPool, org: &str, email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (organization_id, email, display_name, password_hash, role)
         VALUES ($1::uuid, $2, 'Admin', 'x', 'admin') RETURNING id",
    )
    .bind(org)
    .bind(email)
    .fetch_one(pool)
    .await
    .expect("user")
}

fn principal(user_id: Uuid, environment_id: Uuid, role: Role) -> Principal {
    Principal { user_id, environment_id, role, display_name: "test".into(), via: CredentialSource::Token }
}

fn request(name: &str) -> CreateSandbox {
    CreateSandbox {
        name: name.to_string(),
        pack_key: "ubuntu-dev".to_string(),
        image: String::new(),
        image_digest: None,
        command: Vec::new(),
        memory_limit: "512Mi".to_string(),
        cpu_limit: 0.5,
        pids_limit: 256,
        node: None,
        parent_id: None,
        login: None,
    }
}

async fn names(ctx: &ServiceContext, who: &Principal) -> Vec<String> {
    let page = sandbox_service::list(ctx, who, &ListParams::default()).await.expect("list");
    let mut names: Vec<String> = page.items.into_iter().map(|item| item.sandbox.name).collect();
    names.sort();
    names
}

fn profile(subject: &str, email: &str, verified: bool) -> Profile {
    Profile { subject: subject.into(), email: email.into(), email_verified: verified, name: "Ada".into() }
}

#[tokio::test]
async fn a_member_sees_and_works_in_only_the_sandbox_they_were_granted() {
    let Some(owner) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let pool = app_pool().await;
    let ctx = context(&pool);
    let env = uuid(ENVIRONMENT);
    let admin_id = admin_user(&owner, "00000000-0000-0000-0000-000000000001", "root@example.test").await;
    sqlx::query("INSERT INTO user_scopes (user_id, environment_id, role) VALUES ($1, $2, 'admin')")
        .bind(admin_id).bind(env).execute(&owner).await.expect("scope");
    let admin = principal(admin_id, env, Role::Admin);
    let made = user_service::create(&ctx, &admin, "eve@example.test", "Eve", "eves-password-12", "member").await.expect("member");
    let eve = principal(made.id, env, Role::Member);

    let one = sandbox_service::create(&ctx, &admin, request("alpha")).await.expect("alpha");
    let two = sandbox_service::create(&ctx, &admin, request("beta")).await.expect("beta");

    assert!(names(&ctx, &eve).await.is_empty(), "a member sees nothing until a sandbox is handed over");
    tenancy_service::grant(&ctx, &admin, one.id, eve.user_id).await.expect("grant");
    assert_eq!(names(&ctx, &eve).await, vec!["alpha"]);

    // Everything inside the granted sandbox works; the other one does not exist for her.
    assert!(sandbox_service::get(&ctx, &eve, one.id).await.is_ok());
    assert!(sandbox_service::act(&ctx, &eve, one.id, sandbox_service::LifecycleAction::Stop).await.is_ok());
    assert!(sandbox_service::get(&ctx, &eve, two.id).await.is_err());
    assert!(sandbox_service::delete(&ctx, &eve, two.id).await.is_err());

    // Shared things stay out of reach: she cannot hand the sandbox on, invite, or list users.
    assert!(tenancy_service::grant(&ctx, &eve, one.id, admin_id).await.is_err());
    assert!(tenancy_service::invite(&ctx, &eve, "admin", None, None, 7).await.is_err());
    assert!(user_service::list(&ctx, &eve).await.is_err());

    // Her own sandbox's rules and windows are hers; a foreign sandbox and shared policy are not.
    use nestbox_control_api::services::policy_service;
    assert!(policy_service::allow_destination(&ctx, &eve, one.id, "example.com", Some(443), None).await.is_ok());
    assert!(policy_service::allow_destination(&ctx, &eve, two.id, "example.com", Some(443), None).await.is_err());
    assert!(policy_service::open_window(&ctx, &eve, Some(one.id), 5, "audit", "all", "testing").await.is_ok());
    assert!(policy_service::open_window(&ctx, &eve, Some(two.id), 5, "audit", "all", "testing").await.is_err());
    assert!(policy_service::open_window(&ctx, &eve, None, 5, "audit", "all", "everyone").await.is_err(), "the environment-wide switch is not hers");

    tenancy_service::revoke(&ctx, &admin, one.id, eve.user_id).await.expect("revoke");
    assert!(names(&ctx, &eve).await.is_empty(), "taking the sandbox back closes the door");
    assert!(sandbox_service::get(&ctx, &eve, one.id).await.is_err());

    // A viewer cannot be given a sandbox to work in.
    let viewer = user_service::create(&ctx, &admin, "vic@example.test", "Vic", "vics-password-12", "viewer").await.expect("viewer");
    assert!(tenancy_service::grant(&ctx, &admin, one.id, viewer.id).await.is_err());
}

#[tokio::test]
async fn a_provider_reaches_its_customers_and_a_customer_reaches_only_itself() {
    let Some(owner) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let pool = app_pool().await;
    let ctx = context(&pool);
    let env = uuid(ENVIRONMENT);
    let provider_id = admin_user(&owner, "00000000-0000-0000-0000-000000000001", "provider@example.test").await;
    sqlx::query("INSERT INTO user_scopes (user_id, environment_id, role) VALUES ($1, $2, 'admin')")
        .bind(provider_id).bind(env).execute(&owner).await.expect("scope");
    let provider = principal(provider_id, env, Role::Admin);

    let acme = tenancy_service::create_customer(&ctx, &provider, "Acme GmbH").await.expect("acme");
    let globex = tenancy_service::create_customer(&ctx, &provider, "Globex").await.expect("globex");
    assert_eq!(acme.kind, "customer");
    assert_eq!(acme.environments.len(), 1, "a customer starts with one data center");

    // The provider reaches both customers, as admin.
    let reach: Vec<(Uuid, String)> = sqlx::query_as("SELECT environment_id, role FROM nb_user_environments($1)")
        .bind(provider_id).fetch_all(&owner).await.expect("reach");
    let acme_env = acme.environments[0].id;
    let globex_env = globex.environments[0].id;
    assert!(reach.iter().any(|(e, r)| *e == acme_env && r == "admin"));
    assert!(reach.iter().any(|(e, r)| *e == globex_env && r == "admin"));

    // Each customer's own admin reaches only its own data center.
    let acme_admin = admin_user(&owner, &acme.id.to_string(), "boss@acme.test").await;
    sqlx::query("INSERT INTO user_scopes (user_id, environment_id, role) VALUES ($1, $2, 'admin')")
        .bind(acme_admin).bind(acme_env).execute(&owner).await.expect("scope");
    let own: Vec<Uuid> = sqlx::query_scalar("SELECT environment_id FROM nb_user_environments($1)")
        .bind(acme_admin).fetch_all(&owner).await.expect("own");
    assert_eq!(own, vec![acme_env]);
    assert!(auth_service::principal_for_environment(&owner, acme_admin, "boss", globex_env, CredentialSource::Session).await.is_err());
    assert!(auth_service::principal_for_environment(&owner, acme_admin, "boss", env, CredentialSource::Session).await.is_err());
    assert!(auth_service::principal_for_environment(&owner, provider_id, "provider", globex_env, CredentialSource::Session).await.is_ok());

    // A customer cannot create customers; a customer's data centers are its own business.
    let acme_principal = principal(acme_admin, acme_env, Role::Admin);
    assert!(tenancy_service::create_customer(&ctx, &acme_principal, "Sub").await.is_err());
    let second = tenancy_service::create_data_center(&ctx, &acme_principal, "Frankfurt").await.expect("data center");
    let own: Vec<Uuid> = sqlx::query_scalar("SELECT environment_id FROM nb_user_environments($1)")
        .bind(acme_admin).fetch_all(&owner).await.expect("own");
    assert!(own.contains(&second.id) && own.len() == 2);

    // The tree the console shows is the same reach.
    let seen = tenancy_service::organizations(&ctx, &acme_principal).await.expect("tree");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].environments.len(), 2);

    // Sites: one is made with the data center; an empty one can go, the last name stays unique.
    let site = tenancy_service::create_site(&ctx, &acme_principal, "Hetzner Falkenstein", "cloud", Some("hetzner"), Some("fsn1")).await.expect("site");
    assert!(tenancy_service::create_site(&ctx, &acme_principal, "Hetzner Falkenstein", "cloud", None, None).await.is_err());
    assert!(tenancy_service::create_site(&ctx, &acme_principal, "Odd", "moon", None, None).await.is_err());
    assert_eq!(tenancy_service::list_sites(&ctx, &acme_principal).await.expect("sites").len(), 2);
    tenancy_service::delete_site(&ctx, &acme_principal, site.id).await.expect("delete");
}

#[tokio::test]
async fn sign_in_by_identity_provider_registers_invites_and_refuses_strangers() {
    let Some(owner) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let pool = app_pool().await;
    let ctx = context(&pool);
    let saas = config(DeployMode::Saas);
    let hosted = config(DeployMode::SelfHosted);

    // A stranger on a self-hosted installation is turned away; one with an unverified email too.
    assert!(oidc_service::resolve(&ctx, &hosted, "google", &profile("s1", "new@example.test", true), None).await.is_err());
    assert!(oidc_service::resolve(&ctx, &saas, "google", &profile("s2", "new@example.test", false), None).await.is_err());

    // In saas mode the first sign-in starts an onboarding: organisation, data center, site.
    let (user_id, _) = oidc_service::resolve(&ctx, &saas, "google", &profile("s3", "new@example.test", true), None).await.expect("register");
    let reach: Vec<(Uuid, String)> = sqlx::query_as("SELECT environment_id, role FROM nb_user_environments($1)")
        .bind(user_id).fetch_all(&owner).await.expect("reach");
    assert_eq!(reach.len(), 1);
    assert_eq!(reach[0].1, "admin");
    let sites: i64 = sqlx::query_scalar("SELECT count(*) FROM sites WHERE environment_id = $1")
        .bind(reach[0].0).fetch_one(&owner).await.expect("sites");
    assert_eq!(sites, 1);
    let kind: String = sqlx::query_scalar("SELECT o.kind FROM users u JOIN organizations o ON o.id = u.organization_id WHERE u.id = $1")
        .bind(user_id).fetch_one(&owner).await.expect("kind");
    assert_eq!(kind, "customer");
    // The same identity signs in again as the same person; a password login is not possible.
    let (again, _) = oidc_service::resolve(&ctx, &saas, "google", &profile("s3", "new@example.test", true), None).await.expect("again");
    assert_eq!(again, user_id);
    assert!(auth_service::login(&owner, "new@example.test", "anything-at-all", 60, 60).await.is_err());
    // An address that already has an account is not taken over by a new identity.
    assert!(oidc_service::resolve(&ctx, &saas, "google", &profile("s4", "new@example.test", true), None).await.is_err());

    // An invitation to one sandbox: the person becomes a member and sees exactly that sandbox.
    let env = reach[0].0;
    let owner_principal = principal(user_id, env, Role::Admin);
    let kept = sandbox_service::create(&ctx, &owner_principal, request("kept")).await.expect("kept");
    sandbox_service::create(&ctx, &owner_principal, request("private")).await.expect("private");
    let invitation = tenancy_service::invite(&ctx, &owner_principal, "admin", Some(kept.id), Some("guest@example.test"), 7).await.expect("invite");
    assert_eq!(invitation.invitation.role, "member", "a sandbox invitation is always a member's");
    let preview = tenancy_service::preview(&ctx, &invitation.token).await.expect("preview");
    assert_eq!(preview.sandbox.as_deref(), Some("kept"));
    assert!(tenancy_service::preview(&ctx, "not-a-token").await.is_err());

    let id: Uuid = invitation.invitation.id;
    // Another address cannot use an invitation made for one.
    assert!(oidc_service::resolve(&ctx, &hosted, "google", &profile("g1", "thief@example.test", true), Some(id)).await.is_err());
    let (guest, _) = oidc_service::resolve(&ctx, &hosted, "google", &profile("g2", "guest@example.test", true), Some(id)).await.expect("accept");
    let role = principal(guest, env, Role::Member);
    assert_eq!(names(&ctx, &role).await, vec!["kept"]);
    // Used once.
    assert!(tenancy_service::preview(&ctx, &invitation.token).await.is_err());
    assert!(oidc_service::resolve(&ctx, &hosted, "google", &profile("g3", "other@example.test", true), Some(id)).await.is_err());
}

#[tokio::test]
async fn nodes_belong_to_a_tenant_and_a_sandbox_name_is_unique_on_the_installation() {
    let Some(owner) = connect().await else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let pool = app_pool().await;
    let ctx = context(&pool);
    let env = uuid(ENVIRONMENT);
    let provider_id = admin_user(&owner, "00000000-0000-0000-0000-000000000001", "p@example.test").await;
    sqlx::query("INSERT INTO user_scopes (user_id, environment_id, role) VALUES ($1, $2, 'admin')")
        .bind(provider_id).bind(env).execute(&owner).await.expect("scope");
    let provider = principal(provider_id, env, Role::Admin);
    let acme = tenancy_service::create_customer(&ctx, &provider, "Acme").await.expect("acme");
    let globex = tenancy_service::create_customer(&ctx, &provider, "Globex").await.expect("globex");
    let acme_env = acme.environments[0].id;
    let globex_env = globex.environments[0].id;
    let acme_admin = principal(admin_user(&owner, &acme.id.to_string(), "a@acme.test").await, acme_env, Role::Admin);
    let globex_admin = principal(admin_user(&owner, &globex.id.to_string(), "g@globex.test").await, globex_env, Role::Admin);

    // A new customer has no node: the provider's local node is not theirs to use.
    let refused = sandbox_service::create(&ctx, &acme_admin, request("first")).await.unwrap_err();
    assert!(refused.message.contains("Add a node"), "{}", refused.message);

    // Each customer brings a node of their own.
    for (name, environment) in [("acme-node", acme_env), ("globex-node", globex_env)] {
        sqlx::query(
            "INSERT INTO nodes (name, display_name, environment_id, kind, runner_url) VALUES ($1, $1, $2, 'vm', 'http://10.77.0.2:9000')",
        )
        .bind(name).bind(environment).execute(&owner).await.expect("node");
    }
    let made = sandbox_service::create(&ctx, &acme_admin, request("shared-name")).await.expect("acme sandbox");
    assert_eq!(made.node_name.as_deref(), Some("acme-node"), "a lone own node is the default");
    // Naming another tenant's node is a not-found, not a refusal that confirms it exists.
    let mut elsewhere = request("probe");
    elsewhere.node = Some("globex-node".into());
    assert!(sandbox_service::create(&ctx, &acme_admin, elsewhere).await.is_err());
    // The same name in another data center is refused, on any node.
    let clash = sandbox_service::create(&ctx, &globex_admin, request("shared-name")).await.unwrap_err();
    assert!(clash.message.contains("another data center"), "{}", clash.message);
    assert!(sandbox_service::create(&ctx, &globex_admin, request("other-name")).await.is_ok());
    // Neither sees the other's sandboxes.
    assert_eq!(names(&ctx, &acme_admin).await, vec!["shared-name"]);
    assert_eq!(names(&ctx, &globex_admin).await, vec!["other-name"]);
}
