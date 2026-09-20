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

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut output = stdout();
        // Disable mouse reporting even when the app unwinds from a panic. The
        // ratatui panic hook restores raw mode and the alternate screen, but it
        // does not know that boxpigma enabled mouse capture separately.
        // Leave the kitty keyboard protocol before anything else prints: a terminal
        // left in it would feed the shell `CSI u` encodings instead of plain keys.
        let _ = boxpigma::utils::terminal::disable_terminal_modes(&mut output);
        // Hand the cursor shape back to whatever the user configured.
        let _ = execute!(output, cursor::SetCursorStyle::DefaultUserShape);
        let _ = execute!(output, DisableMouseCapture, ResetColor, cursor::Show);
        // On the panic path the ratatui panic hook has already called restore(),
        // so only call it on clean exits to avoid restoring the terminal twice.
        if !std::thread::panicking() {
            ratatui::restore();
        }
        let _ = write!(output, "\r\n");
        let _ = output.flush();
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
    // Mouse capture is what makes the player bar clickable, and also what stops the terminal
    // from selecting text; `mouse = false` gives the selection back.
    if app.config.mouse {
        execute!(stdout(), EnableMouseCapture)?;
    }
    execute!(stdout(), app.config.cursor_style.command())?;
    // Kitty keyboard protocol + bracketed paste; both are no-ops where unsupported.
    boxpigma::utils::terminal::enable_terminal_modes(&mut stdout())?;
    app.run(terminal).await
}
