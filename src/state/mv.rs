//! The music video of the song that is playing: its poster and what the API knows about it,
//! drawn in a column of the lyrics page.
//!
//! One MV at a time, because one song plays at a time: the slot is filled when a track starts
//! and dropped when the next one starts. The download cannot run on the event loop, so the
//! slot is shared with the task that fills it — the same shape the topbar's portrait uses
//! (`state::avatar`) and the player bar's cover (`playback::CoverState`). Nothing is written to
//! disk: `cache/covers.rs` holds covers, and a poster is fetched once per play.
//!
//! Every failure is silent by design: a song with no MV, a detail request that fails, a cover
//! URL that 403s, a request that times out, bytes that are not an image — all of them leave the
//! slot empty, and the lyrics page then draws exactly what it drew before this module existed.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

use ncm_api::{MvInfo, NcmClient};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use reqwest::Client;

/// Pixels the API is asked for, through its own `?param=` resize parameter — the one the
/// artist portrait and the topbar's face use. The poster is drawn over at most two dozen cells,
/// so 480 square covers a hidpi cell grid with room to spare without asking for a frame the
/// panel is never going to draw.
pub const POSTER_PIXELS: u32 = 480;

/// A decoded poster and the MV it belongs to.
pub struct MvPanel {
    pub info: MvInfo,
    pub poster: StatefulProtocol,
}

/// The poster the frame draws, or nothing while it is still on its way — and after a load that
/// failed.
static PANEL: Mutex<Option<MvPanel>> = Mutex::new(None);

/// The song the slot belongs to. [`clear`] bumps it, so a poster that started loading for an
/// earlier track is dropped when it lands instead of being drawn beside the new song's lyrics.
///
/// A counter rather than the id the cover's slot nearby compares against, because the panel is
/// dropped more often than it is filled: a track with no MV clears the slot and starts no load,
/// so there would be no new id to compare a late result against. Bumping on every track change
/// makes "cleared, and nothing was asked for in its place" reject the late poster of its own
/// accord, without the slot having to tell the two cases apart — the rule the topbar's portrait
/// follows for its session.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The slot, ignoring a poisoned lock: a panic that happened while a poster was being installed
/// says nothing about the frame that wants to draw one.
fn slot() -> MutexGuard<'static, Option<MvPanel>> {
    PANEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The panel for the frame that is being drawn. `None` until the poster has landed, and when
/// the song has no MV or the load failed: the caller draws nothing.
pub fn panel() -> MutexGuard<'static, Option<MvPanel>> {
    slot()
}

/// Forget the poster: the song it belonged to is over. A load already in flight is dropped when
/// it lands, because it carries the song it was started for.
pub fn clear() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    *slot() = None;
}

/// The song a load belongs to. Taken before the load starts, checked when it lands.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Install a poster that just finished loading, unless the song it was fetched for is over.
/// Returns whether it went in — which is what the caller repaints for.
pub fn install(generation: u64, panel: MvPanel) -> bool {
    if GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    *slot() = Some(panel);
    true
}

/// Fetch the MV behind `mv_id` and install its poster for `generation`, if the song it belongs
/// to is still the one playing by the time the bytes are decoded. `false` — and no state change
/// — for every way this can fail: no MV id, a detail request that errors, a cover that cannot be
/// downloaded, bytes that do not decode.
pub async fn fetch_and_install(
    client: &NcmClient,
    http: &Client,
    mv_id: u64,
    picker: &Picker,
    generation: u64,
) -> bool {
    // A track that was skipped before this load started gets no request at all.
    if mv_id == 0 || GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    let Ok(info) = client.mv_detail(mv_id).await else {
        return false;
    };
    match load_poster(http, &info.cover, picker).await {
        Some(poster) => install(generation, MvPanel { info, poster }),
        None => false,
    }
}

/// Download and decode the poster behind `url`, the way the artist page loads a singer's: ask
/// the API to resize it, then decode off the async runtime.
pub async fn load_poster(http: &Client, url: &str, picker: &Picker) -> Option<StatefulProtocol> {
    if url.is_empty() {
        return None;
    }
    let url = format!("{url}?param={POSTER_PIXELS}y{POSTER_PIXELS}");
    let bytes = http.get(url).send().await.ok()?.bytes().await.ok()?;
    let picker = picker.clone();

    // Decoding is CPU work on a byte buffer; the resize itself happens at draw time.
    tokio::task::spawn_blocking(move || decode_poster(&bytes, &picker))
        .await
        .ok()?
}

/// Decode poster bytes into the rectangle the panel draws. No mask and no crop: a poster is a
/// rectangle, where the portrait on the bar and the cover on the disc are circles. `None` when
/// the bytes are not an image at all.
pub fn decode_poster(bytes: &[u8], picker: &Picker) -> Option<StatefulProtocol> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(picker.new_resize_protocol(image))
}

/// Test fixtures for the tests that fill the one slot: they take turns at it, and the poster
/// they fill it with is built here once instead of in each of them. The lyrics page's panel
/// tests read the same strings back off the screen, so they are named here rather than written
/// twice.
#[cfg(test)]
pub(crate) mod fixtures {
    use image::{Rgb, RgbImage};

    use super::*;

    pub(crate) const NAME: &str = "MV 标题";
    pub(crate) const ARTIST: &str = "MV 歌手";
    /// 3:45, so the panel's own line spells its own length.
    pub(crate) const DURATION_MS: u64 = 225_000;
    pub(crate) const PUBLISH_TIME: &str = "1993-09-09";
    /// Long enough to need the rows left under the facts, and then to be cut by them.
    pub(crate) const DESC: &str = "简介：这是一段足够长的简介，长到面板下面留给它的那几行放不下，\
                                    所以它必须被截断，绝不能溢出面板、写到边框外面去。";

    /// The panel is one slot for the whole process, so the tests that write it take turns at
    /// it. It is a tokio mutex because the tests that fetch hold their turn across a request;
    /// nothing heavy runs under it — what is awaited there is a refused connection.
    static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    pub(crate) async fn turn() -> tokio::sync::MutexGuard<'static, ()> {
        TURN.lock().await
    }

    /// The MV as the API answers one, over a poster as the API serves one: a JPEG that decodes
    /// into the panel's rectangle — as many pixels as the real request asks for, because the
    /// panel draws the image at its own size rather than stretching it. The gradient is steep in
    /// both directions so the cells it paints cannot be mistaken for text.
    pub(crate) fn panel(picker: &Picker) -> MvPanel {
        let image = RgbImage::from_fn(POSTER_PIXELS, POSTER_PIXELS, |x, y| {
            Rgb([
                (x * 255 / (POSTER_PIXELS - 1)) as u8,
                (y * 255 / (POSTER_PIXELS - 1)) as u8,
                60,
            ])
        });
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image)
            .expect("the fixture encodes");

        MvPanel {
            info: MvInfo {
                id: 7,
                name: NAME.to_string(),
                artist_id: 99,
                artist_name: ARTIST.to_string(),
                cover: "http://127.0.0.1:9/poster.jpg".to_string(),
                duration: DURATION_MS,
                publish_time: PUBLISH_TIME.to_string(),
                desc: DESC.to_string(),
                play_count: 0,
                sub_count: 0,
                share_count: 0,
                like_count: 0,
                comment_count: 0,
                resolutions: Vec::new(),
            },
            poster: decode_poster(&bytes, picker).expect("and decodes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{fixtures::turn, *};

    /// A picker for a terminal without a graphics protocol: halfblocks is what every terminal
    /// can draw, so a poster is never invisible to the tests.
    fn picker() -> Picker {
        Picker::halfblocks()
    }

    /// A cover client the way the app builds one — connect deadline, no total deadline beyond
    /// it. Nothing here ever reaches a server: port 9 is the discard port, and nothing listens
    /// on it.
    fn http() -> Client {
        Client::builder()
            .connect_timeout(Duration::from_millis(250))
            .build()
            .expect("client")
    }

    /// The song has no MV, so there is nothing to fetch and nothing to install — which is what
    /// an id of `0` means everywhere else in the app. No request is made either.
    #[tokio::test]
    async fn a_song_without_an_mv_installs_nothing() {
        let _turn = turn().await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = NcmClient::new().expect("client");

        clear();
        let generation = generation();

        assert!(
            !fetch_and_install(&client, &http(), 0, &picker(), generation).await,
            "a song with no MV must not report a panel"
        );
        assert!(panel().is_none(), "and must not leave one behind");
    }

    /// Skipping to the next track forgets the poster, and a load that was already on its way
    /// when that happened lands in the empty slot instead of putting the previous song's poster
    /// beside the new song's lyrics.
    #[tokio::test]
    async fn a_poster_from_a_previous_song_is_dropped() {
        let _turn = turn().await;
        let generation = generation();
        let in_flight = fixtures::panel(&picker());

        clear();

        assert!(
            !install(generation, in_flight),
            "a finished song must not be served"
        );
        assert!(panel().is_none());
    }

    /// A panel that was installed for the song playing now is the panel the frame draws.
    #[tokio::test]
    async fn the_poster_of_the_current_song_is_the_one_installed() {
        let _turn = turn().await;

        clear();
        assert!(install(generation(), fixtures::panel(&picker())));

        let slot = panel();
        let installed = slot.as_ref().expect("the poster went in");
        assert_eq!(installed.info.name, fixtures::NAME);
        assert_eq!(installed.info.id, 7);
    }

    /// A detail request that cannot be answered is not a special case: the slot stays as empty
    /// as it was, and the page goes on drawing what it drew. The proxy points at the discard
    /// port, so the request fails the way it fails on a machine that is offline.
    #[tokio::test]
    async fn a_detail_that_cannot_be_reached_installs_nothing() {
        let _turn = turn().await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = NcmClient::builder()
            .proxy("http://127.0.0.1:9")
            .timeout(Duration::from_millis(500))
            .build()
            .expect("client");

        clear();
        let generation = generation();

        assert!(!fetch_and_install(&client, &http(), 1, &picker(), generation).await);
        assert!(panel().is_none(), "a failed detail must not fill the slot");
    }

    /// A cover URL that cannot answer, and one that is not a URL at all, are the same answer:
    /// nothing to draw. Bytes that are not an image agree.
    #[tokio::test]
    async fn a_poster_that_cannot_be_downloaded_is_no_poster() {
        let _turn = turn().await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let picker = picker();
        let http = http();

        clear();

        for url in ["http://127.0.0.1:9/poster.jpg", ""] {
            assert!(
                load_poster(&http, url, &picker).await.is_none(),
                "{url:?} must not report a poster"
            );
        }
        assert!(decode_poster(b"not an image", &picker).is_none());
        assert!(panel().is_none(), "no failure may leave a poster behind");
    }
}
