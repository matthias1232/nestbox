//! What the commands are, and what each one asks the API for.

use serde_json::Value;

use crate::api::Api;
use crate::{print, Failure};

/// The commands, in the order `nestbox help` prints them.
const USAGE: &str = "\
nestbox supervisor list
nestbox supervisor status <sandbox>
nestbox supervisor settings <sandbox>
nestbox supervisor set-settings <sandbox> <json>
nestbox supervisor check <sandbox>
nestbox supervisor apply-policy <sandbox> <policy-hash>
nestbox supervisor restart-agent <sandbox>
nestbox sandbox list
nestbox sandbox logs <sandbox> [tail]
nestbox sandbox start <sandbox>
nestbox sandbox stop <sandbox>";

#[tokio::main]
pub async fn run(args: &[String]) -> Result<(), Failure> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();

    // Help is answered before a login: asking for the usage of a tool should
    // never need a credential.
    if matches!(words.as_slice(), [] | ["help"]) {
        println!("{USAGE}");
        return Ok(());
    }

    let api = Api::connect().await?;

    match words.as_slice() {
        ["supervisor", "list"] => print_list(&api, "/api/v1/supervisors").await,
        ["supervisor", "status", sandbox] => {
            let id = sandbox_id(&api, sandbox).await?;
            let body = api.get(&format!("/api/v1/sandboxes/{id}/settings")).await?;
            let supervisor = supervisor_of(&api, sandbox).await?;
            print(&serde_json::json!({
                "supervisor": supervisor,
                "settings": body.get("settings").cloned().unwrap_or(Value::Null),
            }));
            Ok(())
        }
        ["supervisor", "settings", sandbox] => {
            let id = sandbox_id(&api, sandbox).await?;
            print(&api.get(&format!("/api/v1/sandboxes/{id}/settings")).await?);
            Ok(())
        }
        ["supervisor", "set-settings", sandbox, json] => {
            let id = sandbox_id(&api, sandbox).await?;
            let document = parse_json(json)?;
            print(&api.put(&format!("/api/v1/sandboxes/{id}/settings"), document).await?);
            Ok(())
        }
        ["supervisor", "check", sandbox] | ["supervisor", "restart-agent", sandbox] => {
            let kind = if words.get(1) == Some(&"check") { "run_check" } else { "restart_agent" };
            act(&api, sandbox, kind, Value::Null).await
        }
        ["supervisor", "apply-policy", sandbox, hash] => {
            act(&api, sandbox, "apply_policy", serde_json::json!({ "policy_hash": hash })).await
        }
        ["sandbox", "list"] => print_list(&api, "/api/v1/sandboxes").await,
        ["sandbox", "logs", sandbox] => {
            let id = sandbox_id(&api, sandbox).await?;
            print(&api.get(&format!("/api/v1/sandboxes/{id}/logs?tail=200")).await?);
            Ok(())
        }
        ["sandbox", "start", sandbox] => lifecycle(&api, sandbox, "start").await,
        ["sandbox", "stop", sandbox] => lifecycle(&api, sandbox, "stop").await,
        _ => Err(Failure::usage(format!("unknown command\n\n{USAGE}"))),
    }
}

/// Print the `items` of a collection, one per line when they are objects.
async fn print_list(api: &Api, path: &str) -> Result<(), Failure> {
    let body = api.get(path).await?;
    let items = body.get("items").cloned().unwrap_or(body);
    print(&items);
    Ok(())
}

/// Ask the supervisor of one sandbox to run a whitelisted action.
async fn act(api: &Api, sandbox: &str, kind: &str, payload: Value) -> Result<(), Failure> {
    let id = sandbox_id(api, sandbox).await?;
    let body = serde_json::json!({ "kind": kind, "payload": payload });
    print(&api.post(&format!("/api/v1/sandboxes/{id}/supervisor/actions"), Some(body)).await?);
    Ok(())
}

/// Start or stop a sandbox through the lifecycle route.
async fn lifecycle(api: &Api, sandbox: &str, action: &str) -> Result<(), Failure> {
    let id = sandbox_id(api, sandbox).await?;
    let body = serde_json::json!({ "action": action });
    print(&api.post(&format!("/api/v1/sandboxes/{id}/actions"), Some(body)).await?);
    Ok(())
}

/// Resolve a sandbox name to its id without paging the whole list.
async fn sandbox_id(api: &Api, name: &str) -> Result<String, Failure> {
    let body = api
        .get(&format!("/api/v1/sandboxes?search={name}&limit=50"))
        .await?;
    let found = body
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some(name))
        })
        .ok_or_else(|| Failure::failed(format!("no sandbox named '{name}'")))?;
    found
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Failure::failed("the sandbox row carries no id"))
}

/// The supervisor row that watches a sandbox, or a clear failure.
async fn supervisor_of(api: &Api, sandbox: &str) -> Result<Value, Failure> {
    let body = api.get("/api/v1/supervisors").await?;
    let found = body
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("name").and_then(Value::as_str) == Some(sandbox)
                    || item.get("sandbox_id").and_then(Value::as_str) == Some(sandbox)
            })
        })
        .cloned();
    match found {
        Some(row) => Ok(row),
        None => Err(Failure::failed(format!(
            "no supervisor reports for '{sandbox}' (the API shows {:?})",
            body.get("items").and_then(Value::as_array).map(Vec::len)
        ))),
    }
}

/// A settings document from the command line.
fn parse_json(text: &str) -> Result<Value, Failure> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| Failure::usage(format!("settings are not JSON: {error}")))?;
    if !value.is_object() {
        return Err(Failure::usage("settings must be a JSON object"));
    }
    Ok(value)
}
