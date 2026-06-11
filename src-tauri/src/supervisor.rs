//! Drives a local Hindsight instance through the `hindsight-embed` CLI.
//!
//! The desktop app is a thin supervisor with two actions: **start** the daemon
//! (always-on) and **open the control center**. Everything else — profile/LLM
//! config, `.env` editing, stop/restart, control-plane UI, ports, log tail — now
//! lives in hindsight-embed's bundled control center web app (`control start`),
//! so the menu just launches it. Liveness is read off the daemon's `/health`.
//!
//! It runs under its own dedicated embed profile ([`PROFILE`]) pinned to a fixed
//! port, so it never collides with the user's default profile or a dev server.
//!
//! Nothing is bundled. To launch `hindsight-embed` the app prefers an installed
//! binary (`HINDSIGHT_EMBED_BIN`, then well-known locations) and otherwise falls
//! back to `uvx hindsight-embed`. Child processes get an augmented PATH because
//! Finder-launched apps inherit only a minimal one.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Dedicated embed profile this app owns end-to-end.
const PROFILE: &str = "desktop";
/// Fixed daemon port for [`PROFILE`]. In the named-profile range (8889-9888) and
/// clear of the default profile / dev server on 8888.
pub const DAEMON_PORT: u16 = 8899;
/// Package spec used with `uvx` when no installed binary is found.
const EMBED_PKG: &str = "hindsight-embed";
/// Default control-center web-app port (overridable via HINDSIGHT_EMBED_CONTROL_PORT).
const CONTROL_PORT_DEFAULT: u16 = 7878;

/// Disables the daemon's idle auto-exit (`idle_timeout <= 0`) so it stays up.
const ALWAYS_ON_ENV: (&str, &str) = ("HINDSIGHT_EMBED_DAEMON_IDLE_TIMEOUT", "0");

/// True when the daemon answers `GET /health` with a 200 on [`DAEMON_PORT`].
pub fn health_ok() -> bool {
    let timeout = Duration::from_millis(1500);
    let Ok(mut stream) = TcpStream::connect_timeout(&local(DAEMON_PORT), timeout) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let req = b"GET /health HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if stream.write_all(req).is_err() {
        return false;
    }
    let mut buf = [0u8; 64];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    // Status line looks like "HTTP/1.0 200 OK".
    let head = String::from_utf8_lossy(&buf[..n]);
    head.starts_with("HTTP/1.") && head.contains(" 200")
}

/// The running daemon's API version (from `GET /version`), if reachable.
pub fn api_version() -> Option<String> {
    let body = http_get_raw(DAEMON_PORT, "/version")?;
    // Tiny parse of {"api_version":"X.Y.Z", ...} — avoids a JSON dependency.
    let key = "\"api_version\":\"";
    let start = body.find(key)? + key.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// GET a localhost path and return the full raw HTTP response (headers + body).
/// HTTP/1.0 + `Connection: close` lets `read_to_end` capture the whole reply.
fn http_get_raw(port: u16, path: &str) -> Option<String> {
    let timeout = Duration::from_millis(1500);
    let mut stream = TcpStream::connect_timeout(&local(port), timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let req = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Start the daemon with auto-exit disabled. Blocking: the first run can take
/// 1-3 minutes while `uvx` fetches hindsight-api and loads models — call from a
/// background thread, never the menu-event (main) thread.
pub fn daemon_start() -> bool {
    ensure_profile();
    run_embed(&["daemon", "start"], &[ALWAYS_ON_ENV])
}

/// Open hindsight-embed's control center deep-linked to our [`PROFILE`]. We start
/// it with `--no-open` (idempotent; ensures the server + access token exist) and
/// open the tokenized, profile-scoped URL ourselves, since the CLI's own browser
/// open isn't profile-aware. Blocking until the server is ready — call from a
/// background thread.
pub fn open_control_center() {
    run_embed(&["control", "start", "--no-open"], &[]);
    let port = control_port();
    let url = match control_token() {
        Some(t) => format!("http://localhost:{port}/?token={t}&profile={PROFILE}"),
        None => format!("http://localhost:{port}/?profile={PROFILE}"),
    };
    let _ = open::that(url);
}

/// Control-center port: `HINDSIGHT_EMBED_CONTROL_PORT` or the default 7878.
fn control_port() -> u16 {
    std::env::var("HINDSIGHT_EMBED_CONTROL_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|p| *p >= 1024)
        .unwrap_or(CONTROL_PORT_DEFAULT)
}

/// The control center's access token (persisted by `control start`), if present.
fn control_token() -> Option<String> {
    let path = dirs::home_dir()?.join(".hindsight").join("control.token");
    let token = std::fs::read_to_string(path).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Create the dedicated profile if absent; `--merge` makes this idempotent and
/// preserves any config the user has set via the control center.
fn ensure_profile() {
    let port = DAEMON_PORT.to_string();
    run_embed(
        &["profile", "create", PROFILE, "--port", &port, "--merge"],
        &[],
    );
}

fn local(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn run_embed(args: &[&str], extra_env: &[(&str, &str)]) -> bool {
    let mut cmd = embed_command();
    cmd.args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    matches!(cmd.output(), Ok(out) if out.status.success())
}

/// Build the base command that runs `hindsight-embed` under [`PROFILE`]: an
/// installed binary if we can find one, otherwise `uvx hindsight-embed`. Always
/// runs with an augmented PATH so `uvx`/`npx` (and the nested `uvx
/// hindsight-api`) resolve even when the app was launched from Finder.
fn embed_command() -> Command {
    let mut cmd = match resolved_embed_bin() {
        Some(bin) => Command::new(bin),
        None => {
            let mut c = Command::new("uvx");
            c.arg(EMBED_PKG);
            c
        }
    };
    cmd.env("HINDSIGHT_EMBED_PROFILE", PROFILE);
    cmd.env("PATH", child_path());
    cmd
}

/// Locate an installed `hindsight-embed`. `None` means "fall back to uvx".
fn resolved_embed_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HINDSIGHT_EMBED_BIN") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin/hindsight-embed"));
        candidates.push(home.join(".pyenv/shims/hindsight-embed"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/hindsight-embed"));
    candidates.push(PathBuf::from("/usr/local/bin/hindsight-embed"));
    candidates.into_iter().find(|p| p.exists())
}

/// PATH for child processes: well-known install dirs prepended to the inherited
/// PATH (Finder-launched apps get a bare `/usr/bin:/bin`).
fn child_path() -> OsString {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/bin"));
        paths.push(home.join(".cargo/bin"));
        paths.push(home.join(".pyenv/shims"));
    }
    paths.push(PathBuf::from("/opt/homebrew/bin"));
    paths.push(PathBuf::from("/usr/local/bin"));
    paths.push(PathBuf::from("/usr/bin"));
    paths.push(PathBuf::from("/bin"));
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}
