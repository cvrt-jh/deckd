use crate::error::{DeckError, Result};
use std::collections::HashMap;
use tracing::{debug, warn};

/// Execute an HTTP request.
pub async fn execute(
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let mut builder = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        other => {
            return Err(DeckError::Action(format!(
                "unsupported HTTP method: {other}"
            )));
        }
    };

    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(body) = body {
        builder = builder.body(body.to_string());
    }

    let resp = builder.send().await?;
    let status = resp.status();

    if status.is_success() {
        debug!("HTTP {method} {url} → {status}");
    } else {
        warn!("HTTP {method} {url} → {status}");
    }

    Ok(())
}
