//! Where the IPC endpoint lives, and how a byte stream is opened to it.
//!
//! Transport is platform-specific: a Unix domain socket at
//! `~/.cache/boxpigma/boxpigma.sock` on Linux/macOS, and a named pipe `\\.\pipe\boxpigma`
//! on Windows. The endpoint is user-scoped so no authentication is needed.
//!
//! The protocol never learns which of the two it is talking to: the listener yields an
//! `AcceptedStream` and that `AsyncRead + AsyncWrite` is all the server loop ever names.

#[cfg(unix)]
use std::fs;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[cfg(unix)]
use crate::utils::boxpigma_cache_dir;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::{AcceptedStream, ClientStream, restrict_socket};
#[cfg(windows)]
pub(super) use windows::{AcceptedStream, ClientStream, PIPE_NAME};

/// Socket file name inside `boxpigma_cache_dir()` (Unix only).
pub const SOCKET_FILE: &str = "boxpigma.sock";

fn socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        boxpigma_cache_dir().join(SOCKET_FILE)
    }
    #[cfg(windows)]
    {
        PathBuf::from(PIPE_NAME)
    }
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
    #[cfg(unix)]
    {
        ClientStream::connect(path).await
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new().open(path.to_string_lossy().as_ref())
    }
}

/// Platform-specific listener for the IPC server. On Windows a named pipe is
/// re-created for every connection, so `next()` holds the pipe name rather than
/// a persistent handle.
pub(super) enum IpcListener {
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
    #[cfg(windows)]
    Pipe { name: String },
}

/// Bind the listener, clearing any stale file left by a previous run on Unix.
/// Returns `None` when another boxpigma instance already holds the endpoint.
impl IpcListener {
    pub(super) fn bind() -> Option<Self> {
        let path = resolve_socket_path();
        #[cfg(unix)]
        {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            match tokio::net::UnixListener::bind(&path) {
                Ok(listener) => {
                    restrict_socket(&path);
                    Some(Self::Unix(listener))
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
                    listener.map(Self::Unix)
                }
            }
        }
        #[cfg(windows)]
        {
            let name = path.to_string_lossy().into_owned();
            match tokio::net::windows::named_pipe::ServerOptions::new().create(&name) {
                Ok(_) => Some(Self::Pipe { name }),
                Err(e) => {
                    // Windows releases the pipe name when the owning process
                    // exits, so a failed bind always means a live instance.
                    log::warn!("ipc: another boxpigma instance already owns the pipe {name}: {e}");
                    None
                }
            }
        }
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    pub(super) async fn next(&mut self) -> Option<AcceptedStream> {
        match self {
            #[cfg(unix)]
            Self::Unix(l) => l.accept().await.ok().map(|(stream, _)| stream),
            #[cfg(windows)]
            Self::Pipe { name } => {
                // A fresh server instance per connection; after a client
                // attaches, that instance becomes the connection stream.
                let server = tokio::net::windows::named_pipe::ServerOptions::new()
                    .create(name)
                    .ok()?;
                server.connect().await.ok()?;
                Some(server)
            }
        }
    }
}

/// Removes the Unix socket file on drop (clean shutdown of the TUI).
/// On Windows the OS releases the pipe name automatically, so nothing to do.
pub struct IpcServerGuard {
    #[cfg_attr(windows, allow(dead_code))]
    remove_on_drop: bool,
}

impl IpcServerGuard {
    pub(super) fn new(remove_on_drop: bool) -> Self {
        Self { remove_on_drop }
    }
}

impl Drop for IpcServerGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.remove_on_drop {
            let _ = fs::remove_file(resolve_socket_path());
        }
    }
}

/// Remove the Unix socket file unconditionally (used on shutdown paths where
/// the guard may already be dropped). No-op on Windows.
pub fn remove_socket() {
    #[cfg(unix)]
    let _ = fs::remove_file(resolve_socket_path());
}
