use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) fn socket_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("KRYPTIC_SOCKET_PATH") {
        if !override_path.is_empty() {
            return PathBuf::from(override_path);
        }
    }
    if cfg!(target_os = "linux") {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            if !runtime_dir.is_empty() {
                return Path::new(&runtime_dir).join("kryptic-daemon.sock");
            }
        }
    }
    // Same per-user directory the daemon listens on (PROTOCOL.md). Never /tmp.
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return Path::new(&home)
                    .join("Library/Application Support/kryptic/kryptic-daemon.sock");
            }
        }
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        if !config_home.is_empty() {
            return Path::new(&config_home).join("kryptic/kryptic-daemon.sock");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Path::new(&home).join(".config/kryptic/kryptic-daemon.sock");
        }
    }
    PathBuf::from("kryptic-daemon.sock")
}

pub(crate) fn round_trip(line: &[u8], timeout: Duration) -> std::io::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(socket_path())?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(line)?;

    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte)?;
        if n == 0 {
            break;
        }
        buffer.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    Ok(buffer)
}
