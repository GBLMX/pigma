//! `cargo test --release --lib -- --ignored --nocapture frame_bench`
//!
//! What an **idle** frame costs. The other benches answer "does the work that runs *while music
//! plays* stay far below its frame budget"; this one answers the question the main loop raises:
//! it draws one whole frame per iteration and `handle_events` returns at least every 32 ms, so
//! this number is paid around 31 times a second **whether or not anything on screen changed**.

use ratatui::{Terminal, backend::TestBackend};

use crate::{app::App, config::Config, state::Page};

#[tokio::test]
#[ignore]
async fn one_idle_frame_costs() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut app = App::new(Config::default(), false).expect("app");
    // The main page with no song playing: the state a user leaves the app in.
    app.state.navigation.page = Page::Main;

    let mut terminal = Terminal::new(TestBackend::new(200, 50)).expect("backend");
    let per_frame = crate::bench_util::time("空闲整帧（主页面，无播放）", 300, || {
        terminal.draw(|f| super::draw(f, &mut app)).expect("draw");
    });
    // The loop is event-driven (`handle_events` blocks on the event stream unless the user is
    // dragging the seek bar), so this is NOT paid continuously while idle: it is what one
    // frame costs, and the analysis stream spends it about 30 times a second while playing.
    println!(
        "  → 单帧开销；播放时分析流约 30 次/秒 → 约 {:.2}% 单核",
        crate::bench_util::core_share(per_frame, 30.0)
    );
}
