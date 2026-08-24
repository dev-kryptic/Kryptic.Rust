use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn socket_path() -> PathBuf {
    if let Ok(override_path) = std::env::var("KRYPTIC_SOCKET_PATH") {
        if !override_path.is_empty() {
            return PathBuf::from(override_path);
        }
    }
    PathBuf::from(r"\\.\pipe\kryptic-daemon")
}

pub(crate) fn round_trip(line: &[u8], timeout: Duration) -> std::io::Result<Vec<u8>> {
    let path = socket_path();
    let path_str = path.to_string_lossy();
    if !path_str.starts_with(r"\\.\pipe\") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "unix sockets are not used on Windows; set KRYPTIC_SOCKET_PATH to a named pipe",
        ));
    }

    let deadline = Instant::now() + timeout;
    let mut pipe = loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => break file,
            Err(error) => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("timed out connecting to the daemon pipe: {error}"),
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    };

    pipe.write_all(line)?;

    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = pipe.read(&mut byte)?;
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
