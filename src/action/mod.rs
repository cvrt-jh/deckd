pub mod http;
pub mod navigate;
pub mod shell;

use crate::config::schema::ActionConfig;
use crate::error::Result;
use crate::event::DeckEvent;
use tokio::sync::broadcast;
use tracing::info;

/// Execute an action based on its config.
///
/// # Errors
/// Returns `DeckError` if the action fails (HTTP error, shell failure, etc.).
pub async fn execute(action: &ActionConfig, tx: &broadcast::Sender<DeckEvent>) -> Result<()> {
    match action {
        ActionConfig::Http {
            method,
            url,
            headers,
            body,
        } => {
            info!("executing HTTP {method} {url}");
            http::execute(method, url, headers, body.as_deref()).await
        }
        ActionConfig::Shell { command } => {
            info!("executing shell: {command}");
            shell::execute(command).await
        }
        ActionConfig::Navigate { page } => {
            info!("navigating to page: {page}");
            let _ = tx.send(DeckEvent::NavigateTo(page.clone()));
            Ok(())
        }
        ActionConfig::Back => {
            info!("navigating back");
            let _ = tx.send(DeckEvent::NavigateBack);
            Ok(())
        }
        ActionConfig::Home => {
            info!("navigating home");
            let _ = tx.send(DeckEvent::NavigateHome);
            Ok(())
        }
    }
}
