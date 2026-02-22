use crate::error::{DeckError, Result};
use tracing::{debug, warn};

/// Execute a shell command via `/bin/sh -c`.
///
/// # Errors
/// Returns `DeckError::Io` if the command cannot be spawned,
/// or `DeckError::Shell` if it exits with a non-zero status.
pub async fn execute(command: &str) -> Result<()> {
    let output = tokio::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            debug!("shell output: {stdout}");
        }
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("shell command failed (exit {}): {stderr}", output.status);
        Err(DeckError::Shell {
            command: command.to_string(),
            message: stderr.to_string(),
        })
    }
}
