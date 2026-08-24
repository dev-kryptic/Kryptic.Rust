use super::*;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, MutexGuard};
use std::thread;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[allow(unused_unsafe)]
fn unset_env(key: &str) {
    unsafe {
        env::remove_var(key);
    }
}

fn setup_project(_guard: &MutexGuard<'_, ()>) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kryptic-test-{}", std::process::id()));
    let unique = dir.join(format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&unique).unwrap();
    fs::write(
        unique.join("kryptic.json"),
        r#"{"projectId":"proj_test123456"}"#,
    )
    .unwrap();
    env::set_current_dir(&unique).unwrap();
    set_env("KRYPTIC_SILENT", "true");
    for key in [
        "KRYPTIC_DISABLED",
        "KRYPTIC_PROJECT_ID",
        "KRYPTIC_ENV",
        "INJECTED_KEY",
        "EXISTING_KEY",
        "RUST_ENV",
        "APP_ENV",
        "ENVIRONMENT",
        "ENV",
    ] {
        unset_env(key);
    }
    unique
}

#[cfg(unix)]
fn start_mock_daemon(handler: fn(Value) -> Value) -> std::path::PathBuf {
    use std::os::unix::net::UnixListener;

    let dir = std::env::temp_dir().join(format!("kd-{}", std::process::id()));
    fs::create_dir_all(&dir).ok();
    let socket = dir.join(format!(
        "d-{}.sock",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).unwrap();
    set_env(
        "KRYPTIC_SOCKET_PATH",
        socket.to_str().expect("socket path utf-8"),
    );

    thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let Ok(request) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let body = serde_json::to_vec(&handler(request)).unwrap();
            let _ = stream.write_all(&body);
            let _ = stream.write_all(b"\n");
        }
    });

    socket
}

#[test]
fn noop_when_disabled() {
    let _guard = lock_env();
    setup_project(&_guard);
    set_env("KRYPTIC_DISABLED", "true");

    let result = inject();

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("disabled"));
}

#[test]
fn noop_in_production() {
    let _guard = lock_env();
    setup_project(&_guard);
    set_env("RUST_ENV", "production");

    let result = inject();

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("rust_env_production"));
}

#[cfg(unix)]
mod unix {
    use super::*;

    #[test]
    fn injects_secrets() {
        let _guard = lock_env();
        setup_project(&_guard);
        start_mock_daemon(|request| {
            assert_eq!(request["projectId"], "proj_test123456");
            assert_eq!(request["environment"], "development");
            json!({
                "v": 1,
                "ok": true,
                "secrets": [{"key": "INJECTED_KEY", "value": "from-daemon"}]
            })
        });

        let result = inject();

        assert!(!result.skipped);
        assert_eq!(result.injected, 1);
        assert_eq!(env::var("INJECTED_KEY").unwrap(), "from-daemon");
    }

    #[test]
    fn never_overwrites_existing_variables() {
        let _guard = lock_env();
        setup_project(&_guard);
        set_env("EXISTING_KEY", "real-env-wins");
        start_mock_daemon(|_| {
            json!({
                "v": 1,
                "ok": true,
                "secrets": [{"key": "EXISTING_KEY", "value": "x"}]
            })
        });

        let result = inject();

        assert_eq!(result.injected, 0);
        assert_eq!(env::var("EXISTING_KEY").unwrap(), "real-env-wins");
    }

    #[test]
    fn noop_when_daemon_missing() {
        let _guard = lock_env();
        setup_project(&_guard);
        let missing = std::env::temp_dir().join("kryptic-missing.sock");
        set_env("KRYPTIC_SOCKET_PATH", missing.to_str().unwrap());

        let result = inject();

        assert!(result.skipped);
        assert_eq!(result.reason.as_deref(), Some("daemon_unreachable"));
    }

    #[test]
    fn handles_error_responses() {
        let _guard = lock_env();
        setup_project(&_guard);
        start_mock_daemon(|_| json!({"v": 1, "ok": false, "error": "access_denied"}));

        let result = inject();

        assert!(result.skipped);
        assert_eq!(result.reason.as_deref(), Some("access_denied"));
    }

    #[test]
    fn env_overrides_win() {
        let _guard = lock_env();
        setup_project(&_guard);
        set_env("KRYPTIC_PROJECT_ID", "proj_override0001");
        set_env("KRYPTIC_ENV", "staging");

        start_mock_daemon(|request| {
            assert_eq!(request["projectId"], "proj_override0001");
            assert_eq!(request["environment"], "staging");
            json!({"v": 1, "ok": true, "secrets": []})
        });

        inject();
    }
}
