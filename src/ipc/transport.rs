//! Where the IPC endpoint lives, and how a byte stream is opened to it.
//!
//! A Unix domain socket at `<cache dir>/boxpigma.sock`, where the cache dir is
//! `boxpigma_cache_dir()` (on Linux `~/.cache/boxpigma`). Its permissions are restricted to
//! mode 0600, and the endpoint is user-scoped so no authentication is needed.
//!
//! The protocol never learns what the endpoint is: the listener yields an
//! `AcceptedStream` and that `AsyncRead + AsyncWrite` is all the server loop ever names.

use std::fs;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::utils::boxpigma_cache_dir;

mod unix;

pub(super) use unix::{AcceptedStream, ClientStream, restrict_socket};

/// Socket file name inside `boxpigma_cache_dir()`.
pub const SOCKET_FILE: &str = "boxpigma.sock";

fn socket_path() -> PathBuf {
    boxpigma_cache_dir().join(SOCKET_FILE)
}

thread_local! {
    static SOCKET_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Process-wide socket-path override (set once by the CLI's `--socket` flag).
/// The thread-local test override, when present, still takes precedence.
static SOCKET_GLOBAL: OnceLock<PathBuf> = OnceLock::new();

/// Override the socket path for this thread (used by integration tests, which
/// each bind their own socket so they can run in parallel). Safe because the
/// override is thread-local.
#[doc(hidden)]
pub fn set_socket_path_override(path: Option<PathBuf>) {
    SOCKET_OVERRIDE.with(|c| *c.borrow_mut() = path);
}

/// Override the socket path process-wide (used by the CLI `--socket` flag so a
/// daemon and the `status`/`msg` commands can address a non-default instance).
pub fn set_socket_path(path: Option<PathBuf>) {
    if let Some(p) = path {
        let _ = SOCKET_GLOBAL.set(p);
    }
}

/// Resolve the socket path: a thread-local override if set, otherwise the
/// process-wide override, otherwise the default location under `boxpigma_cache_dir()`.
pub(super) fn resolve_socket_path() -> PathBuf {
    SOCKET_OVERRIDE
        .with(|c| c.borrow().clone())
        .unwrap_or_else(|| SOCKET_GLOBAL.get().cloned().unwrap_or_else(socket_path))
}

/// Connect to the running instance's listener endpoint.
pub(super) async fn client_connect(path: &Path) -> std::io::Result<ClientStream> {
    ClientStream::connect(path).await
}

/// Listener for the IPC server.
pub(super) struct IpcListener(tokio::net::UnixListener);

/// Bind the listener, clearing any stale file left by a previous run.
/// Returns `None` when another boxpigma instance already holds the endpoint.
impl IpcListener {
    pub(super) fn bind() -> Option<Self> {
        let path = resolve_socket_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match tokio::net::UnixListener::bind(&path) {
            Ok(listener) => {
                restrict_socket(&path);
                Some(Self(listener))
            }
            Err(_) => {
                // Either a live instance owns the socket or it is stale.
                // A non-blocking connect probe tells us which: if we can
                // connect, another instance is running and we must not
                // steal the socket.
                if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                    log::warn!(
                        "ipc: another boxpigma instance already owns {}",
                        path.display()
                    );
                    return None;
                }
                let _ = fs::remove_file(&path);
                let listener = tokio::net::UnixListener::bind(&path).ok();
                if listener.is_some() {
                    restrict_socket(&path);
                }
                listener.map(Self)
            }
        }
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    pub(super) async fn next(&mut self) -> Option<AcceptedStream> {
        self.0.accept().await.ok().map(|(stream, _)| stream)
    }
}

/// Removes the socket file on drop (clean shutdown of the TUI).
pub struct IpcServerGuard {
    remove_on_drop: bool,
}

impl IpcServerGuard {
    pub(super) fn new(remove_on_drop: bool) -> Self {
        Self { remove_on_drop }
    }
}

impl Drop for IpcServerGuard {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_file(resolve_socket_path());
        }
    }
}

/// Remove the socket file unconditionally (used on shutdown paths where
/// the guard may already be dropped).
pub fn remove_socket() {
    let _ = fs::remove_file(resolve_socket_path());
}
