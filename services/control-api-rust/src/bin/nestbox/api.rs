//! The HTTP side of the CLI: one client, one session, the same rules as the
//! console.

use reqwest::{Client, Method, Response};
use serde_json::Value;

use crate::Failure;

/// A logged-in client.
pub struct Api {
    client: Client,
    base: String,
    csrf: String,
    cookies: String,
}

impl Api {
    /// Log in with the credentials the environment carries.
    pub async fn connect() -> Result<Self, Failure> {
        let base = std::env::var("NESTBOX_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        let base = base.trim_end_matches('/').to_string();
        let email = std::env::var("NESTBOX_EMAIL")
            .map_err(|_| Failure::usage("NESTBOX_EMAIL is not set"))?;
        let password = std::env::var("NESTBOX_PASSWORD")
            .map_err(|_| Failure::usage("NESTBOX_PASSWORD is not set"))?;

        let client = Client::builder()
            .build()
            .map_err(|error| Failure::failed(format!("no HTTP client: {error}")))?;

        let response = client
            .post(format!("{base}/api/v1/auth/login"))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await
            .map_err(|error| Failure::failed(format!("login failed: {error}")))?;

        let cookies = cookie_header(&response);
        let body: Value = response
            .json()
            .await
            .map_err(|error| Failure::failed(format!("login answered nothing readable: {error}")))?;
        let csrf = body
            .get("csrf_token")
            .and_then(Value::as_str)
            .ok_or_else(|| Failure::failed("login did not return a csrf token"))?
            .to_string();

        Ok(Self { client, base, csrf, cookies })
    }
}

impl Api {
    /// `GET path` and return the parsed answer.
    pub async fn get(&self, path: &str) -> Result<Value, Failure> {
        self.send(Method::GET, path, None).await
    }

    /// `POST path` with a JSON body (or none) and return the answer.
    pub async fn post(&self, path: &str, body: Option<Value>) -> Result<Value, Failure> {
        self.send(Method::POST, path, body).await
    }

    /// `PUT path` with a JSON body and return the answer.
    pub async fn put(&self, path: &str, body: Value) -> Result<Value, Failure> {
        self.send(Method::PUT, path, Some(body)).await
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, Failure> {
        let mut request = self
            .client
            .request(method.clone(), format!("{}{}", self.base, path))
            .header("cookie", self.cookies.clone())
            .header("x-csrf-token", self.csrf.clone());
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| Failure::failed(format!("{method} {path}: {error}")))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| Failure::failed(format!("{method} {path}: {error}")))?;
        let value: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| serde_json::json!({ "raw": text }));
        if !status.is_success() {
            let code = value
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("request.failed");
            let message = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("the request failed");
            return Err(Failure::failed(format!("{code}: {message}")));
        }
        Ok(value)
    }
}

/// The refreshable session cookie, as a header value.
fn cookie_header(response: &Response) -> String {
    response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}
