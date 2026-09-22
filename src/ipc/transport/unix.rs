//! The Unix half of the transport: a domain socket under `boxpigma_cache_dir()`, owned by
//! the user who started the daemon.

use std::fs;

/// The stream a client connects with (a Unix socket).
#[cfg(unix)]
pub(crate) type ClientStream = tokio::net::UnixStream;

/// The stream accepted by the server (the same Unix socket, one per connection).
#[cfg(unix)]
pub(crate) type AcceptedStream = tokio::net::UnixStream;

/// Restrict the IPC socket to its owner. The endpoint is user-scoped (see the module docs),
/// so no other account needs access; the process umask would otherwise leave it reachable
/// by group/other.
#[cfg(unix)]
pub(crate) fn restrict_socket(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        log::warn!(
            "ipc: failed to restrict permissions of {}: {e}",
            path.display()
        );
    }
}
