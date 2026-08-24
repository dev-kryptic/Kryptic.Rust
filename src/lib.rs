//! The Kryptic daemon client for Rust. During development startup, [`inject`]
//! fetches the current project's secrets from the local Kryptic daemon and
//! sets them as environment variables. Outside development it is a no-op.
//! It never panics: a missing daemon means the application simply starts with
//! the environment it already has.
//!
//! Protocol: daemon/PROTOCOL.md v1 (newline-delimited JSON over a local socket).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
mod transport_unix;
#[cfg(unix)]
use transport_unix::round_trip;

#[cfg(windows)]
mod transport_windows;
#[cfg(windows)]
use transport_windows::round_trip;

const PROTOCOL_VERSION: u32 = 1;

/// What [`inject`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectResult {
    pub injected: usize,
    pub skipped: bool,
    pub reason: Option<String>,
}

/// Fetches secrets from the daemon and sets them with [`env::set_var`].
/// Existing environment variables are never overwritten.
pub fn inject() -> InjectResult {
    match inject_inner() {
        Ok(result) => result,
        Err(error) => {
            warn(&format!(
                "daemon not reachable ({error}) - continuing without injected secrets."
            ));
            InjectResult {
                injected: 0,
                skipped: true,
                reason: Some("daemon_unreachable".into()),
            }
        }
    }
}

fn inject_inner() -> Result<InjectResult, String> {
    if let Some(reason) = should_skip() {
        return Ok(InjectResult {
            injected: 0,
            skipped: true,
            reason: Some(reason),
        });
    }

    let config = find_kryptic_json();

    let project_id = env::var("KRYPTIC_PROJECT_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| config.as_ref().and_then(|item| item.0.clone()));
    let Some(project_id) = project_id else {
        warn("no kryptic.json found (and no KRYPTIC_PROJECT_ID set) - nothing to inject.");
        return Ok(InjectResult {
            injected: 0,
            skipped: true,
            reason: Some("no_project".into()),
        });
    };

    let environment = env::var("KRYPTIC_ENV")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| config.as_ref().and_then(|item| item.1.clone()))
        .unwrap_or_else(|| "development".into());

    let payload = serde_json::json!({
        "v": PROTOCOL_VERSION,
        "type": "secrets",
        "projectId": project_id,
        "environment": environment,
    });
    let mut line = payload.to_string();
    line.push('\n');

    let response_line = round_trip(line.as_bytes(), timeout()).map_err(|error| error.to_string())?;
    let response: serde_json::Value =
        serde_json::from_slice(&response_line).map_err(|error| error.to_string())?;

    if response.get("ok").and_then(|value| value.as_bool()) != Some(true) {
        let error = response
            .get("error")
            .and_then(|value| value.as_str())
            .unwrap_or("internal");
        let message = response
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        warn(&format!(
            "daemon refused the request ({error}): {message}"
        ));
        return Ok(InjectResult {
            injected: 0,
            skipped: true,
            reason: Some(error.to_string()),
        });
    }

    let mut injected = 0usize;
    if let Some(secrets) = response.get("secrets").and_then(|value| value.as_array()) {
        for secret in secrets {
            let Some(key) = secret.get("key").and_then(|value| value.as_str()) else {
                continue;
            };
            if key.is_empty() || env::var_os(key).is_some() {
                continue;
            }
            let value = secret
                .get("value")
                .and_then(|item| item.as_str())
                .unwrap_or("");
            set_env(key, value);
            injected += 1;
        }
    }

    Ok(InjectResult {
        injected,
        skipped: false,
        reason: None,
    })
}

fn should_skip() -> Option<String> {
    if env::var("KRYPTIC_DISABLED").ok().as_deref() == Some("true") {
        return Some("disabled".into());
    }

    for variable in ["RUST_ENV", "APP_ENV", "ENVIRONMENT", "ENV"] {
        if let Ok(value) = env::var(variable) {
            let lower = value.to_ascii_lowercase();
            if matches!(lower.as_str(), "production" | "prod" | "staging") {
                return Some(format!("{}_{lower}", variable.to_ascii_lowercase()));
            }
        }
    }

    None
}

fn timeout() -> Duration {
    env::var("KRYPTIC_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_millis(2000))
}

/// Walks up from the working directory looking for kryptic.json.
/// Returns (projectId, defaultEnvironment).
fn find_kryptic_json() -> Option<(Option<String>, Option<String>)> {
    let mut directory: PathBuf = env::current_dir().ok()?;
    loop {
        let candidate = directory.join("kryptic.json");
        if candidate.is_file() {
            return parse_kryptic_json(&candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

fn parse_kryptic_json(path: &Path) -> Option<(Option<String>, Option<String>)> {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(_) => return None,
    };
    match serde_json::from_str::<serde_json::Value>(&data) {
        Ok(value) => Some((
            value
                .get("projectId")
                .and_then(|item| item.as_str())
                .map(str::to_string),
            value
                .get("defaultEnvironment")
                .and_then(|item| item.as_str())
                .map(str::to_string),
        )),
        Err(_) => {
            warn(&format!("could not parse {} - ignoring it.", path.display()));
            None
        }
    }
}

fn warn(message: &str) {
    if env::var("KRYPTIC_SILENT").ok().as_deref() == Some("true") {
        return;
    }
    eprintln!("[kryptic] {message}");
}

#[allow(unused_unsafe)]
fn set_env(key: &str, value: &str) {
    // Called once at process startup, before the host application reads config.
    unsafe {
        env::set_var(key, value);
    }
}

#[cfg(test)]
mod tests;
