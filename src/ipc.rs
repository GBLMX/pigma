//! IPC between the running TUI and the `boxpigma status` / `boxpigma msg` CLI
//! commands.
//!
//! The TUI binds a listener and accepts one-line JSON requests:
//!
//! - `{"cmd":"status"}` → the server replies with a serialized `StatusSnapshot`.
//! - `{"cmd":"subscribe"}` → the server streams each snapshot change as a JSON
//!   line until the connection closes (event push for waybar / other clients).
//! - `{"cmd":"msg","action":...}` → the server forwards an `IpcEvent` into the
//!   app's event channel and replies `{"ok":true}`.
//! - `{"cmd":"capabilities"}` → the server replies with its self-describing
//!   contract: the API version, the program version, every `msg` action it
//!   implements (`ACTIONS`) and the endpoint it listens on. Read-only, so it
//!   answers even before login or with nothing loaded.
//!
//! Transport is platform-specific: a Unix domain socket at
//! `~/.cache/boxpigma/boxpigma.sock` on Linux/macOS, and a named pipe `\\.\pipe\boxpigma`
//! on Windows. The endpoint is user-scoped so no authentication is needed.

#[cfg(unix)]
use std::fs;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use color_eyre::eyre::{OptionExt, WrapErr};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{broadcast, mpsc},
};

#[cfg(unix)]
use crate::utils::boxpigma_cache_dir;
use crate::{
    event::{AppEvent, Event},
    playback::PlayMode,
};

/// Socket file name inside `boxpigma_cache_dir()` (Unix only).
pub const SOCKET_FILE: &str = "boxpigma.sock";

/// Default named-pipe name on Windows.
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\boxpigma";

/// Request sent from the CLI to the running TUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    Status,
    /// Return the current playback queue (`boxpigma status -L`).
    List,
    /// Return the self-describing contract (`boxpigma msg capabilities`): the
    /// API version, the program version, the [`ACTIONS`] catalogue and the
    /// endpoint in use. Read-only, so it answers without login or playback.
    #[serde(alias = "caps")]
    Capabilities,
    /// Keep the connection open and stream each `StatusSnapshot` change as a
    /// JSON line. An initial snapshot is sent immediately on connect.
    Subscribe,
    /// Search songs on NetEase Cloud Music
    /// (`boxpigma msg search <keyword>`). The server replies with a JSON array of
    /// [`SearchEntry`]; results are registered in-process so a returned id can
    /// later be played with `boxpigma msg play <id>`.
    Search {
        keyword: String,
    },
    Msg {
        action: MsgAction,
    },
}

/// A playback control action for `boxpigma msg`.
///
/// The accepted names are [`ACTIONS`]: each variant's canonical name plus the
/// `alias`es below, which mirror that table's aliases — the
/// `every_listed_action_name_dispatches` test fails if the two drift apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MsgAction {
    #[serde(alias = "prev")]
    Previous,
    Next,
    Pause,
    /// Resume when paused, start when stopped. With `song_id` set, jump to that
    /// song in the active queue and play it (`boxpigma msg play <id>`).
    Play {
        song_id: Option<u64>,
    },
    /// Play/pause toggle (the TUI spacebar semantics: start when stopped,
    /// resume when paused, pause when playing).
    #[serde(alias = "play_pause", alias = "toggle-play", alias = "play-pause")]
    TogglePlay,
    /// Exactly one of `delta` / `absolute` is set:
    /// - `delta`: fraction of 0..=1 to add/subtract (mirrors the TUI's `+`/`-`).
    /// - `absolute`: target fraction of 0..=1.
    Volume {
        delta: Option<f64>,
        absolute: Option<f64>,
    },
    Mode,
    Like,
    Dislike,
    #[serde(alias = "unlike", alias = "toggle")]
    ToggleLike,
    /// Dynamically switch the daemon's queue to another endpoint. `endpoint` is
    /// an API endpoint name (e.g. `toplist`, `liked`); `playlist` optionally
    /// picks the 1-based playlist within list-type endpoints.
    #[serde(alias = "switch-list", alias = "switch")]
    SwitchList {
        endpoint: String,
        playlist: Option<usize>,
    },
}

/// Runtime event dispatched to the app loop for a `msg` action.
#[derive(Debug, Clone)]
pub enum IpcEvent {
    Previous,
    Next,
    Pause,
    Play {
        song_id: Option<u64>,
    },
    TogglePlay,
    Volume {
        delta: Option<f64>,
        absolute: Option<f64>,
    },
    Mode,
    Like,
    Dislike,
    ToggleLike,
    SwitchList {
        endpoint: String,
        playlist: Option<usize>,
    },
}

impl From<MsgAction> for IpcEvent {
    fn from(action: MsgAction) -> Self {
        match action {
            MsgAction::Previous => IpcEvent::Previous,
            MsgAction::Next => IpcEvent::Next,
            MsgAction::Pause => IpcEvent::Pause,
            MsgAction::Play { song_id } => IpcEvent::Play { song_id },
            MsgAction::TogglePlay => IpcEvent::TogglePlay,
            MsgAction::Volume { delta, absolute } => IpcEvent::Volume { delta, absolute },
            MsgAction::Mode => IpcEvent::Mode,
            MsgAction::Like => IpcEvent::Like,
            MsgAction::Dislike => IpcEvent::Dislike,
            MsgAction::ToggleLike => IpcEvent::ToggleLike,
            MsgAction::SwitchList { endpoint, playlist } => {
                IpcEvent::SwitchList { endpoint, playlist }
            }
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                          `msg` action catalogue                           */
/* -------------------------------------------------------------------------- */

/// Contract version reported as `api` by [`capabilities`].
///
/// Bump it only when an action is renamed or removed; a new action, or a new
/// field anywhere in this payload, keeps the same version (see `SKILLS.md`).
pub const API_VERSION: u32 = 1;

/// One entry of the `boxpigma msg` action list: name, aliases, whether the
/// action takes a value argument, a one-line summary, plus the internal
/// dispatch branch.
///
/// [`ACTIONS`] is the only place action names are listed: the `capabilities`
/// payload, the CLI's accepted values and the CLI's dispatch all derive from it
/// (see `src/cli.rs`), so the published list cannot drift from the
/// implementation. The one other appearance is the serde `alias`es on
/// [`MsgAction`] (the wire format); the
/// `every_listed_action_name_dispatches` test keeps the two in sync.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ActionSpec {
    /// Canonical name; also the `name` field of `capabilities`.
    pub name: &'static str,
    /// Equivalent spellings, accepted by both the CLI and the IPC surface.
    pub aliases: &'static [&'static str],
    /// Whether a value argument is accepted (`boxpigma msg <ACTION> <VALUE>`).
    pub takes_value: bool,
    /// One-line description, used by `--help` and `capabilities`.
    pub summary: &'static str,
    /// Dispatch branch; internal, never serialized.
    #[serde(skip)]
    pub(crate) kind: ActionKind,
}

/// What `boxpigma msg` does with an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionKind {
    /// Control action: sent to the daemon as a [`MsgAction`], answered `{"ok":true}`.
    Control(ControlAction),
    /// Request/response action: answered by the daemon with data.
    Query(QueryAction),
}

/// Control actions, one per [`MsgAction`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlAction {
    Previous,
    Next,
    Pause,
    Play,
    TogglePlay,
    Volume,
    Mode,
    Like,
    Dislike,
    ToggleLike,
    SwitchList,
}

/// Request/response actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryAction {
    /// Print the playback queue (with an endpoint argument: switch the queue).
    List,
    /// Search songs.
    Search,
    /// Print the capability contract.
    Capabilities,
}

/// Every `boxpigma msg` action, ascending by name (the order `capabilities`
/// publishes, asserted by `action_catalogue_is_consistent`).
pub const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "capabilities",
        aliases: &["caps"],
        takes_value: false,
        summary: "Print this contract: API version, program version, actions, endpoint",
        kind: ActionKind::Query(QueryAction::Capabilities),
    },
    ActionSpec {
        name: "dislike",
        aliases: &[],
        takes_value: false,
        summary: "Dislike the current song",
        kind: ActionKind::Control(ControlAction::Dislike),
    },
    ActionSpec {
        name: "like",
        aliases: &[],
        takes_value: false,
        summary: "Like the current song",
        kind: ActionKind::Control(ControlAction::Like),
    },
    ActionSpec {
        name: "list",
        aliases: &[],
        takes_value: true,
        summary: "Print the playback queue, or switch it when given an endpoint",
        kind: ActionKind::Query(QueryAction::List),
    },
    ActionSpec {
        name: "mode",
        aliases: &[],
        takes_value: false,
        summary: "Cycle the play mode",
        kind: ActionKind::Control(ControlAction::Mode),
    },
    ActionSpec {
        name: "next",
        aliases: &[],
        takes_value: false,
        summary: "Go to the next song",
        kind: ActionKind::Control(ControlAction::Next),
    },
    ActionSpec {
        name: "pause",
        aliases: &[],
        takes_value: false,
        summary: "Pause playback",
        kind: ActionKind::Control(ControlAction::Pause),
    },
    ActionSpec {
        name: "play",
        aliases: &[],
        takes_value: true,
        summary: "Play/resume, or jump to a song id in the active queue",
        kind: ActionKind::Control(ControlAction::Play),
    },
    ActionSpec {
        name: "previous",
        aliases: &["prev"],
        takes_value: false,
        summary: "Go to the previous song",
        kind: ActionKind::Control(ControlAction::Previous),
    },
    ActionSpec {
        name: "search",
        aliases: &[],
        takes_value: true,
        summary: "Search songs on NetEase Cloud Music",
        kind: ActionKind::Query(QueryAction::Search),
    },
    ActionSpec {
        name: "switch-list",
        aliases: &["switch"],
        takes_value: true,
        summary: "Switch the queue to another endpoint (see --playlist)",
        kind: ActionKind::Control(ControlAction::SwitchList),
    },
    ActionSpec {
        name: "toggle_like",
        aliases: &["unlike", "toggle"],
        takes_value: false,
        summary: "Toggle like on the current song",
        kind: ActionKind::Control(ControlAction::ToggleLike),
    },
    ActionSpec {
        name: "toggle_play",
        aliases: &["play_pause", "toggle-play", "play-pause"],
        takes_value: false,
        summary: "Toggle play/pause (start when stopped, resume when paused)",
        kind: ActionKind::Control(ControlAction::TogglePlay),
    },
    ActionSpec {
        name: "volume",
        aliases: &[],
        takes_value: true,
        summary: "Set the volume (0-100) or adjust it (+5/-5)",
        kind: ActionKind::Control(ControlAction::Volume),
    },
];

/// Look up an action by canonical name or alias.
pub fn action_spec(name: &str) -> Option<&'static ActionSpec> {
    ACTIONS
        .iter()
        .find(|spec| spec.name == name || spec.aliases.contains(&name))
}

/// Payload of the `capabilities` action: what a running instance supports.
#[derive(Debug, Serialize)]
pub struct Capabilities {
    /// Contract version, see [`API_VERSION`].
    pub api: u32,
    /// This program's version.
    pub version: &'static str,
    /// Every action this build implements, ascending by name.
    pub actions: &'static [ActionSpec],
    /// The socket / named pipe this instance listens on.
    pub socket: String,
}

/// Build the `capabilities` payload. Read-only: it reports the compiled-in
/// table, so it never touches login state or playback.
pub fn capabilities() -> Capabilities {
    Capabilities {
        api: API_VERSION,
        version: env!("CARGO_PKG_VERSION"),
        actions: ACTIONS,
        socket: resolve_socket_path().to_string_lossy().into_owned(),
    }
}

/// Live playback state snapshot served to `boxpigma status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub id: u64,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// Total length in milliseconds (0 when nothing is loaded).
    pub duration_ms: u64,
    /// Playback position in milliseconds.
    pub position_ms: u64,
    /// Volume as a fraction of 0..=1.
    pub volume: f64,
    pub playing: bool,
    pub paused: bool,
    /// Stable play-mode key: `sequential` / `repeat_one` / `repeat_all` /
    /// `shuffle` / `heartbeat`.
    pub mode: String,
    pub liked: bool,
}

impl StatusSnapshot {
    pub fn from_playback(state: &crate::playback::PlaybackState) -> Self {
        let song = state.current_song.as_ref();
        let duration_ms = song.map(|s| s.duration).unwrap_or(0);
        let position_ms = song
            .map(|s| (state.progress * s.duration as f64) as u64)
            .unwrap_or(0);
        Self {
            id: song.map(|s| s.id).unwrap_or(0),
            name: song.map(|s| s.name.clone()).unwrap_or_default(),
            artist: song.map(|s| s.singer.clone()).unwrap_or_default(),
            album: song.map(|s| s.album.clone()).unwrap_or_default(),
            duration_ms,
            position_ms,
            volume: state.volume,
            playing: state.playing,
            paused: state.paused,
            mode: mode_key(&state.mode).to_string(),
            liked: state.liked,
        }
    }

    /// Whether `self` differs from `other` in any field that is *not* the
    /// playback position. The app loop compares snapshots with this before
    /// deciding to broadcast, so a running track does not spam subscribers on
    /// every progress tick — position refreshes are instead throttled by time.
    pub fn meaningfully_differs(&self, other: &Self) -> bool {
        self.id != other.id
            || self.name != other.name
            || self.artist != other.artist
            || self.album != other.album
            || self.duration_ms != other.duration_ms
            || self.volume != other.volume
            || self.playing != other.playing
            || self.paused != other.paused
            || self.mode != other.mode
            || self.liked != other.liked
    }
}

fn mode_key(mode: &PlayMode) -> &'static str {
    match mode {
        PlayMode::Sequential => "sequential",
        PlayMode::RepeatOne => "repeat_one",
        PlayMode::RepeatAll => "repeat_all",
        PlayMode::Shuffle => "shuffle",
        PlayMode::Heartbeat { .. } => "heartbeat",
    }
}

/// A single entry in the playback queue, served to `boxpigma status -L`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
}

impl QueueEntry {
    pub fn from_song(song: &ncm_api::SongInfo) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
        }
    }
}

/// Full queue listing served to `boxpigma status -L`: the current song's queue
/// index (0-based, `None` when nothing is queued) plus the songs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub current_index: Option<usize>,
    pub songs: Vec<QueueEntry>,
}

/// A search hit served to `boxpigma msg search <keyword>`. `source` names the
/// provider: always `netease` (NetEase Cloud) now that it is the only search
/// source, kept in the contract because published consumers read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEntry {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub album: String,
    pub duration_ms: u64,
    pub source: String,
}

impl SearchEntry {
    pub fn from_song(song: &ncm_api::SongInfo, source: &str) -> Self {
        Self {
            id: song.id,
            name: song.name.clone(),
            singer: song.singer.clone(),
            album: song.album.clone(),
            duration_ms: song.duration,
            source: source.to_string(),
        }
    }
}

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
fn resolve_socket_path() -> PathBuf {
    SOCKET_OVERRIDE
        .with(|c| c.borrow().clone())
        .unwrap_or_else(|| SOCKET_GLOBAL.get().cloned().unwrap_or_else(socket_path))
}

/// The stream a client connects with (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Connect to the running instance's listener endpoint.
async fn client_connect(path: &Path) -> std::io::Result<ClientStream> {
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
enum IpcListener {
    #[cfg(unix)]
    Unix(tokio::net::UnixListener),
    #[cfg(windows)]
    Pipe { name: String },
}

/// Restrict the IPC socket to its owner. The endpoint is user-scoped (see the module docs),
/// so no other account needs access; the process umask would otherwise leave it reachable
/// by group/other.
#[cfg(unix)]
fn restrict_socket(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        log::warn!(
            "ipc: failed to restrict permissions of {}: {e}",
            path.display()
        );
    }
}

/// Bind the listener, clearing any stale file left by a previous run on Unix.
/// Returns `None` when another boxpigma instance already holds the endpoint.
impl IpcListener {
    fn bind() -> Option<Self> {
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
    async fn next(&mut self) -> Option<AcceptedStream> {
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

/// The stream accepted by the server (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
type AcceptedStream = tokio::net::UnixStream;
#[cfg(windows)]
type AcceptedStream = tokio::net::windows::named_pipe::NamedPipeServer;

/// Start the IPC server for the running TUI.
///
/// Spawns a background task that accepts connections, answering `status` and
/// `list` requests from `status_snapshot` / `queue_snapshot`, streaming
/// snapshot changes to `subscribe` clients via `status_tx`, answering `search`
/// requests with `searcher` and `capabilities` requests from the [`ACTIONS`]
/// catalogue, and forwarding `msg` requests as `IpcEvent`s into `event_tx`.
/// Returns a guard that removes the socket file on drop.
pub fn start_server(
    status_snapshot: Arc<Mutex<StatusSnapshot>>,
    queue_snapshot: Arc<Mutex<QueueSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
    searcher: Arc<crate::app::SearchEngine>,
) -> IpcServerGuard {
    let listener = match IpcListener::bind() {
        Some(l) => l,
        None => return IpcServerGuard::new(false),
    };
    let path = resolve_socket_path();
    let mut listener = listener;
    tokio::spawn(async move {
        loop {
            match listener.next().await {
                Some(stream) => {
                    let snapshot = Arc::clone(&status_snapshot);
                    let queue = Arc::clone(&queue_snapshot);
                    let tx = event_tx.clone();
                    let status_tx = status_tx.clone();
                    let searcher = Arc::clone(&searcher);
                    tokio::spawn(async move {
                        handle_connection(stream, snapshot, queue, status_tx, tx, searcher).await;
                    });
                }
                None => {
                    log::error!("ipc: accept failed");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    });
    log::info!("ipc: listening on {}", path.display());
    IpcServerGuard::new(true)
}

/// Removes the Unix socket file on drop (clean shutdown of the TUI).
/// On Windows the OS releases the pipe name automatically, so nothing to do.
pub struct IpcServerGuard {
    #[cfg_attr(windows, allow(dead_code))]
    remove_on_drop: bool,
}

impl IpcServerGuard {
    fn new(remove_on_drop: bool) -> Self {
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

async fn handle_connection<S>(
    stream: S,
    snapshot: Arc<Mutex<StatusSnapshot>>,
    queue: Arc<Mutex<QueueSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
    searcher: Arc<crate::app::SearchEngine>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut stream = BufReader::new(stream);
    let mut line = String::new();
    if stream.read_line(&mut line).await.is_err() {
        return;
    }
    let request: IpcRequest = match serde_json::from_str(&line) {
        Ok(req) => req,
        Err(e) => {
            log::debug!("ipc: invalid request: {e}");
            return;
        }
    };
    let mut stream = stream.into_inner();
    match request {
        IpcRequest::Status => {
            let reply = {
                let guard = snapshot.lock().unwrap();
                serde_json::to_string(&*guard).unwrap_or_default()
            };
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::List => {
            let reply = {
                let guard = queue.lock().unwrap();
                serde_json::to_string(&*guard).unwrap_or_default()
            };
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::Msg { action } => {
            let event: IpcEvent = action.into();
            let sent = event_tx.send(Event::App(AppEvent::Ipc(event)));
            if sent.is_err() {
                log::error!("ipc: failed to forward msg event: receiver dropped");
            }
            let _ = write_reply(&mut stream, r#"{"ok":true}"#).await;
        }
        IpcRequest::Search { keyword } => {
            let results = searcher.search(&keyword).await;
            let reply = serde_json::to_string(&results).unwrap_or_default();
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::Capabilities => {
            let reply = serde_json::to_string(&capabilities()).unwrap_or_default();
            let _ = write_reply(&mut stream, &reply).await;
        }
        IpcRequest::Subscribe => stream_updates(stream, snapshot, status_tx).await,
    }
}

/// Write a single JSON line (terminated by `\n`) to the client stream.
async fn write_reply<S>(mut stream: S, reply: &str) -> std::io::Result<()>
where
    S: tokio::io::AsyncWrite + Unpin,
{
    let mut framed = reply.to_string();
    framed.push('\n');
    stream.write_all(framed.as_bytes()).await
}

/// `subscribe` mode: send the current snapshot immediately, then stream every
/// broadcast update as a JSON line until the client disconnects or the app
/// shuts the channel down.
async fn stream_updates<S>(
    mut stream: S,
    snapshot: Arc<Mutex<StatusSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
) where
    S: tokio::io::AsyncWrite + Unpin,
{
    let mut rx = status_tx.subscribe();
    let initial = snapshot.lock().unwrap().clone();
    let line = serde_json::to_string(&initial).unwrap_or_default();
    if write_reply(&mut stream, &line).await.is_err() {
        return;
    }
    loop {
        match rx.recv().await {
            Ok(s) => {
                let line = serde_json::to_string(&s).unwrap_or_default();
                if write_reply(&mut stream, &line).await.is_err() {
                    return;
                }
            }
            // A slow subscriber fell behind; resend the current snapshot so it
            // catches up instead of missing the intermediate state.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let current = snapshot.lock().unwrap().clone();
                let line = serde_json::to_string(&current).unwrap_or_default();
                if write_reply(&mut stream, &line).await.is_err() {
                    return;
                }
            }
            // Sender dropped (app quitting) — close the stream.
            Err(_) => return,
        }
    }
}

/// Connect to the running TUI's listener, returning a descriptive error when no
/// instance is up.
async fn connect() -> color_eyre::Result<ClientStream> {
    let path = resolve_socket_path();
    client_connect(&path)
        .await
        .wrap_err("boxpigma is not running (start the TUI or `boxpigma -d`, or check --socket)")
}

/// Send a `status` request and return the live snapshot.
pub async fn fetch_status() -> color_eyre::Result<StatusSnapshot> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"status"}"#)
        .await
        .wrap_err("failed to send status request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read status response")?;
    serde_json::from_str(&buf).wrap_err("invalid status response")
}

/// Send a `list` request and return the live playback queue.
pub async fn fetch_queue() -> color_eyre::Result<QueueSnapshot> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"list"}"#)
        .await
        .wrap_err("failed to send list request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read list response")?;
    serde_json::from_str(&buf).wrap_err("invalid list response")
}

/// Ask the running instance for its contract (`boxpigma msg capabilities`).
/// Returns the reply as-is: the CLI prints it verbatim, so what a script reads
/// from the CLI is what the daemon sent.
pub async fn fetch_capabilities() -> color_eyre::Result<serde_json::Value> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"capabilities"}"#)
        .await
        .wrap_err("failed to send capabilities request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read capabilities response")?;
    parse_capabilities_reply(&buf)
}

/// Parse the reply line of a `capabilities` request.
///
/// An empty line means the instance dropped the request without answering —
/// which is what a build that predates this action does — so say so instead of
/// reporting a JSON syntax error.
fn parse_capabilities_reply(reply: &str) -> color_eyre::Result<serde_json::Value> {
    if reply.trim().is_empty() {
        color_eyre::eyre::bail!(
            "the running instance did not answer `capabilities` (it is probably an older boxpigma; restart it)"
        );
    }
    serde_json::from_str(reply).wrap_err("invalid capabilities response")
}

/// Subscribe to status updates (`{"cmd":"subscribe"}`). Sends the request and
/// returns a line reader over the open connection; every subsequent
/// `StatusSnapshot` change is delivered as one JSON line. The connection stays
/// open until the daemon quits or the stream is dropped.
pub async fn subscribe_status() -> color_eyre::Result<impl tokio::io::AsyncBufRead + Unpin> {
    let mut stream = connect().await?;
    stream
        .write_all(br#"{"cmd":"subscribe"}"#)
        .await
        .wrap_err("failed to send subscribe request")?;
    stream.write_all(b"\n").await?;
    Ok(BufReader::new(stream))
}

/// Send a `search` request (`boxpigma msg search <keyword>`) and return the
/// matching songs, tagged by source and registered in the daemon for a later
/// `boxpigma msg play <id>`.
pub async fn search_songs(keyword: &str) -> color_eyre::Result<Vec<SearchEntry>> {
    let mut stream = connect().await?;
    let request = serde_json::to_string(&IpcRequest::Search {
        keyword: keyword.to_string(),
    })
    .wrap_err("failed to serialize search request")?;
    stream
        .write_all(request.as_bytes())
        .await
        .wrap_err("failed to send search request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read search response")?;
    serde_json::from_str(&buf).wrap_err("invalid search response")
}

/// Send a `msg` action to the running TUI. Returns once the server confirms.
pub async fn send_msg(action: MsgAction) -> color_eyre::Result<()> {
    let mut stream = connect().await?;
    let request = serde_json::to_string(&IpcRequest::Msg { action })
        .wrap_err("failed to serialize msg request")?;
    stream
        .write_all(request.as_bytes())
        .await
        .wrap_err("failed to send msg request")?;
    stream.write_all(b"\n").await?;
    let mut buf = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut buf)
        .await
        .wrap_err("failed to read msg response")?;
    serde_json::from_str::<serde_json::Value>(&buf)
        .ok()
        .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
        .ok_or_eyre("invalid msg response")
        .map(|_| ())
}

/* -------------------------------------------------------------------------- */
/*                                   Testing                                  */
/* -------------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name an action answers to: the canonical one plus its aliases.
    fn names(spec: &ActionSpec) -> impl Iterator<Item = &'static str> {
        std::iter::once(spec.name).chain(spec.aliases.iter().copied())
    }

    /// The smallest request a client can send for `spec` under `name`, in the
    /// same shape `boxpigma msg` puts on the wire.
    fn wire_request(spec: &ActionSpec, name: &str) -> serde_json::Value {
        match spec.kind {
            ActionKind::Query(QueryAction::List) => serde_json::json!({ "cmd": "list" }),
            ActionKind::Query(QueryAction::Search) => {
                serde_json::json!({ "cmd": "search", "keyword": "test" })
            }
            ActionKind::Query(QueryAction::Capabilities) => serde_json::json!({ "cmd": name }),
            ActionKind::Control(ControlAction::SwitchList) => serde_json::json!({
                "cmd": "msg",
                "action": { "action": name, "endpoint": "liked" },
            }),
            ActionKind::Control(_) => {
                serde_json::json!({ "cmd": "msg", "action": { "action": name } })
            }
        }
    }

    /// The catalogue drives the check (rather than a second hand-written list):
    /// every published name and alias is recognized by request dispatch, and an
    /// alias lands on exactly the branch its canonical name lands on.
    #[test]
    fn every_listed_action_name_dispatches() {
        for spec in ACTIONS {
            let canonical: IpcRequest = serde_json::from_value(wire_request(spec, spec.name))
                .unwrap_or_else(|e| panic!("`{}` is not dispatched: {e}", spec.name));
            for name in names(spec) {
                assert_eq!(
                    action_spec(name).map(|s| s.name),
                    Some(spec.name),
                    "`{name}` is missing from the catalogue"
                );
                let request: IpcRequest = serde_json::from_value(wire_request(spec, name))
                    .unwrap_or_else(|e| panic!("dispatch rejects `{name}`: {e}"));
                assert_eq!(
                    request, canonical,
                    "`{name}` does not reach the `{}` branch",
                    spec.name
                );
            }
        }
    }

    /// The catalogue has to be self-consistent: unique names, ascending order
    /// (that order *is* the published one) and a summary for every action.
    #[test]
    fn action_catalogue_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for spec in ACTIONS {
            for name in names(spec) {
                assert!(seen.insert(name), "`{name}` is listed twice");
            }
            assert!(!spec.summary.is_empty(), "`{}` has no summary", spec.name);
        }
        assert!(ACTIONS.windows(2).all(|w| w[0].name < w[1].name));
    }

    #[test]
    fn capabilities_reports_api_version_and_sorted_actions() {
        let json = serde_json::to_value(capabilities()).expect("capabilities must serialize");

        assert_eq!(json["api"], serde_json::json!(API_VERSION));
        assert_eq!(
            json["version"],
            serde_json::json!(env!("CARGO_PKG_VERSION"))
        );
        assert!(!json["socket"].as_str().unwrap_or_default().is_empty());

        let actions = json["actions"].as_array().expect("actions is an array");
        assert_eq!(actions.len(), ACTIONS.len());
        assert!(!actions.is_empty());
        let published: Vec<&str> = actions
            .iter()
            .map(|a| a["name"].as_str().expect("name is a string"))
            .collect();
        assert!(
            published.windows(2).all(|w| w[0] < w[1]),
            "actions must be sorted by name: {published:?}"
        );
        for action in actions {
            assert!(action["aliases"].is_array());
            assert!(action["takes_value"].is_boolean());
            assert!(action["summary"].as_str().is_some_and(|s| !s.is_empty()));
            assert!(
                action.get("kind").is_none(),
                "internal field leaked: {action}"
            );
        }
    }

    /// What the CLI prints for `msg capabilities` is the parsed reply, so a
    /// compact wire line and the pretty output must carry the same JSON.
    #[test]
    fn capabilities_round_trips_over_the_wire() {
        let payload = capabilities();
        let line = serde_json::to_string(&payload).expect("capabilities must serialize");
        assert_eq!(
            parse_capabilities_reply(&line).unwrap(),
            serde_json::to_value(&payload).unwrap()
        );
    }

    /// A build without this action drops the request and answers nothing; that
    /// must not be reported as a JSON syntax error.
    #[test]
    fn capabilities_reply_explains_an_empty_answer() {
        let err = parse_capabilities_reply("\n").expect_err("an empty reply is an error");
        assert!(err.to_string().contains("older boxpigma"), "{err}");
        assert!(parse_capabilities_reply(r#"{"api":1}"#).is_ok());
    }
}
