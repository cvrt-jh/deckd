use std::collections::HashMap;
use tracing::warn;

/// Fetch entity states from Home Assistant for the given entity IDs.
///
/// All requests are made in parallel for fast response.
/// Returns a map of entity_id → state string (e.g. "on", "off", "unavailable").
/// Silently returns an empty map on any error so rendering is never blocked.
pub async fn fetch_ha_states(entities: &[String]) -> HashMap<String, String> {
    if entities.is_empty() {
        return HashMap::new();
    }

    let token = match std::env::var("HA_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => return HashMap::new(),
    };

    let ha_url = std::env::var("HA_URL")
        .unwrap_or_else(|_| "http://homeassistant.local:8123".into());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    // Fire all requests in parallel.
    let futures: Vec<_> = entities
        .iter()
        .map(|entity_id| {
            let url = format!("{ha_url}/api/states/{entity_id}");
            let req = client
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send();
            let eid = entity_id.clone();
            async move {
                match req.await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            if let Some(state) = json.get("state").and_then(|s| s.as_str()) {
                                return Some((eid, state.to_string()));
                            }
                        }
                        None
                    }
                    Ok(resp) => {
                        warn!("HA state fetch {eid}: HTTP {}", resp.status());
                        None
                    }
                    Err(e) => {
                        warn!("HA state fetch {eid}: {e}");
                        None
                    }
                }
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;
    results.into_iter().flatten().collect()
}
