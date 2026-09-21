//! Application entry point: parses CLI arguments, dispatches `status`/`msg`
//! subcommands, and otherwise initializes the terminal and launches the main
//! `App` loop until quit.

use std::io::{Write, stdout};

use boxpigma::cli::{Cli, run_cli};
use clap::{Parser, error::ErrorKind};
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::ResetColor,
};

struct TerminalGuard;

/// Hand the terminal back: control modes off, cursor shape back, then a clean line.
///
/// Split out because two things call it — the guard on a clean exit, and the panic hook
/// installed by [`install_panic_restore_hook`] — and they must not drift apart.
fn restore_terminal_modes() {
    let mut output = stdout();
    // Leave the kitty keyboard protocol before anything else prints: a terminal left in
    // it would feed the shell `CSI u` encodings instead of plain keys. Mouse reporting
    // goes off too — ratatui's panic hook restores raw mode and the alternate screen, but
    // it does not know that boxpigma enabled these separately.
    let _ = boxpigma::utils::terminal::disable_terminal_modes(&mut output);
    // Hand the cursor shape back to whatever the user configured.
    let _ = execute!(output, cursor::SetCursorStyle::DefaultUserShape);
    let _ = execute!(output, DisableMouseCapture, ResetColor, cursor::Show);
    let _ = output.flush();
}

/// Restore the terminal when the process panics.
///
/// [`TerminalGuard`]'s `Drop` cannot do this in a release build: the release profile sets
/// `panic = "abort"` (see `Cargo.toml`), so a panic never unwinds and no `Drop` runs. A
/// panic hook *is* called before the abort, which makes it the only place that can put the
/// terminal back. It chains to the hook `ratatui::init` installed, so raw mode and the
/// alternate screen are restored by ratatui exactly as before.
fn install_panic_restore_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal_modes();
        previous(info);
    }));
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal_modes();
        // On the panic path the ratatui panic hook has already called restore(),
        // so only call it on clean exits to avoid restoring the terminal twice.
        if !std::thread::panicking() {
            ratatui::restore();
        }
        let _ = write!(stdout(), "\r\n");
    }
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();

    // CLI subcommands (`status`/`msg`/`completions`) are one-shot queries; when
    // they fail, print a single clean line instead of the color_eyre trace.
    let app = match run_cli(cli).await {
        Ok(app) => app,
        Err(err) => {
            let msg = err
                .chain()
                .next()
                .map(ToString::to_string)
                .unwrap_or_default();
            clap::Error::raw(ErrorKind::Io, msg)
                .print()
                .expect("failed to write error");
            eprintln!();
            std::process::exit(1);
        }
    };
    let Some(app) = app else {
        return Ok(());
    };

    color_eyre::install()?;
    let terminal = ratatui::init();
    let _terminal_guard = TerminalGuard;
    install_panic_restore_hook();
    // Mouse capture is what makes the player bar clickable, and also what stops the terminal
    // from selecting text; `mouse = false` gives the selection back.
    //
    // Every one of these is best effort: they are conveniences, and on Windows they go
    // through the console API, which does not exist when the process was started without a
    // console (`boxpigma` in a service, under a CI runner, with the terminal replaced by a
    // pipe). A terminal that refuses them is a terminal that does not get mouse support —
    // not a reason to refuse to start.
    if app.config.mouse
        && let Err(e) = execute!(stdout(), EnableMouseCapture)
    {
        log::warn!("mouse capture unavailable: {e}");
    }
    if let Err(e) = execute!(stdout(), app.config.cursor_style.command()) {
        log::warn!("cursor shape unavailable: {e}");
    }
    // Kitty keyboard protocol + bracketed paste; both are no-ops where unsupported.
    if let Err(e) = boxpigma::utils::terminal::enable_terminal_modes(&mut stdout()) {
        log::warn!("terminal modes unavailable: {e}");
    }
    app.run(terminal).await
}
