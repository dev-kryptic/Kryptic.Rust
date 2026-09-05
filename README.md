# kryptic-daemon-client

The Kryptic daemon client for Rust. During development startup it asks the local
Kryptic daemon for the current project's secrets and puts them into the process
environment. Outside development it is a no-op. It never panics - no daemon just
means your app starts with the environment it already has.

```toml
# Cargo.toml
[dependencies]
kryptic-daemon-client = "1.0"
```

```rust
fn main() {
    kryptic::inject(); // call before any env reads

    let db_url = std::env::var("DATABASE_URL").ok();
}
```

## Behavior

- No-op when `RUST_ENV`/`APP_ENV`/`ENVIRONMENT`/`ENV` is production/staging,
  or `KRYPTIC_DISABLED=true`.
- Finds `kryptic.json` by walking up from the working directory.
- Never overwrites environment variables that are already set.
- Configuration via env vars: `KRYPTIC_PROJECT_ID`, `KRYPTIC_ENV`, `KRYPTIC_SOCKET_PATH`,
  `KRYPTIC_TIMEOUT_MS` (default 2000), `KRYPTIC_SILENT`.
- Works on macOS/Linux (unix sockets) and Windows (named pipes).

Protocol: see [daemon/PROTOCOL.md](https://github.com/dev-kryptic/Kryptic.Daemon/blob/main/PROTOCOL.md). License: Apache-2.0.

```bash
cargo test
```
