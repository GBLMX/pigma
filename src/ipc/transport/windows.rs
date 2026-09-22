//! The Windows half of the transport: a named pipe, where the server instance created for a
//! connection becomes that connection.

/// Default named-pipe name on Windows.
#[cfg(windows)]
pub(crate) const PIPE_NAME: &str = r"\\.\pipe\boxpigma";

/// The stream a client connects with (a named pipe).
#[cfg(windows)]
pub(crate) type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// The stream accepted by the server (a named pipe).
#[cfg(windows)]
pub(crate) type AcceptedStream = tokio::net::windows::named_pipe::NamedPipeServer;
