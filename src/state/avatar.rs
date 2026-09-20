//! The signed-in user's own portrait, drawn at the topbar's right end.
//!
//! There is one portrait per session: it is fetched when a login lands, dropped when the
//! session ends, and it is never written to disk — `cache/covers.rs` holds covers, and a face
//! is not one. The download cannot run on the event loop, so the slot is shared with the task
//! that fills it, the same shape the player bar's cover uses (`playback::CoverState`).
//!
//! Every failure is silent by design: a URL that 403s, a request that times out, bytes that
//! are not an image — all of them leave the slot empty, and a topbar without a portrait is a
//! topbar that still says everything it said before.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use reqwest::Client;

/// Pixels the API is asked for. The portrait is drawn over the topbar's three rows, so this
/// covers a hidpi cell grid with room to spare; `?param=` is the API's own resize parameter,
/// the one the artist portrait and `NcmClient::download_img` use.
pub const PIXELS: u32 = 120;

/// The decoded portrait, or nothing while it is still on its way — and after a load that
/// failed.
static PORTRAIT: Mutex<Option<StatefulProtocol>> = Mutex::new(None);

/// The login the slot belongs to. [`clear`] bumps it, so a load that started under an earlier
/// session is dropped when it lands instead of putting that session's face back on the bar.
static SESSION: AtomicU64 = AtomicU64::new(0);

/// The slot, ignoring a poisoned lock: a panic that happened while a portrait was being
/// installed says nothing about the frame that wants to draw one.
fn slot() -> MutexGuard<'static, Option<StatefulProtocol>> {
    PORTRAIT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The portrait for the frame that is being drawn. `None` until the download has landed, and
/// when it failed: the caller draws nothing.
pub fn portrait() -> MutexGuard<'static, Option<StatefulProtocol>> {
    slot()
}

/// Forget the portrait: the session it belonged to is over. A load already in flight is
/// dropped when it lands, because it carries the session it was started for.
pub fn clear() {
    SESSION.fetch_add(1, Ordering::SeqCst);
    *slot() = None;
}

/// The session a load belongs to. Taken before the load starts, checked when it lands.
pub fn session() -> u64 {
    SESSION.load(Ordering::SeqCst)
}

/// Install a portrait that just finished loading, unless the session it was fetched for is
/// over. Returns whether it went in — which is what the caller repaints for.
pub fn install(session: u64, portrait: StatefulProtocol) -> bool {
    if SESSION.load(Ordering::SeqCst) != session {
        return false;
    }
    *slot() = Some(portrait);
    true
}

/// Fetch `url` and install it for `session`. `false` — and no state change — for every way
/// this can fail: no URL, a refused connection, a status error, bytes that do not decode.
pub async fn fetch_and_install(http: &Client, url: &str, picker: &Picker, session: u64) -> bool {
    match load(http, url, picker).await {
        Some(portrait) => install(session, portrait),
        None => false,
    }
}

/// Download and decode the portrait behind `url`, the way the artist page loads a singer's:
/// ask the API to resize it, then decode off the async runtime.
async fn load(http: &Client, url: &str, picker: &Picker) -> Option<StatefulProtocol> {
    if url.is_empty() {
        return None;
    }
    let url = format!("{url}?param={PIXELS}y{PIXELS}");
    let bytes = http.get(url).send().await.ok()?.bytes().await.ok()?;
    let picker = picker.clone();

    // Decoding is CPU work on a byte buffer; the resize itself happens at draw time.
    tokio::task::spawn_blocking(move || decode_circle(&bytes, &picker))
        .await
        .ok()?
}

/// Decode portrait bytes into the circle the topbar draws: cropped to the centre square (the
/// API serves whatever shape the user uploaded) and masked round. `None` when the bytes are
/// not an image at all.
pub fn decode_circle(bytes: &[u8], picker: &Picker) -> Option<StatefulProtocol> {
    let image = image::load_from_memory(bytes).ok()?;
    let (width, height) = image.dimensions();
    let size = width.min(height);
    let mut square = image
        .crop_imm((width - size) / 2, (height - size) / 2, size, size)
        .to_rgba8();
    mask_circle(&mut square);
    Some(picker.new_resize_protocol(DynamicImage::ImageRgba8(square)))
}

/// Cut `image` down to a disc: everything outside the inscribed circle turns transparent, so
/// the bar draws a round face rather than a square one. The same cut the player bar makes for
/// its cover, which is private to the disc it draws.
pub fn mask_circle(image: &mut RgbaImage) {
    let (width, height) = image.dimensions();
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = cx.min(cy);

    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - cx;
        let dy = y as f32 + 0.5 - cy;
        if dx * dx + dy * dy > radius * radius {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    }
}

/// Test fixtures for the tests that fill the one slot: they take turns at it, and the picture
/// they fill it with is built here once instead of in each of them.
#[cfg(test)]
pub(crate) mod fixtures {
    use image::{Rgb, RgbImage};

    use super::*;

    /// The portrait is one slot for the whole process, so the tests that write it take turns
    /// at it. It is a tokio mutex because the test that fetches holds its turn across the
    /// request; nothing heavy runs under it — what is awaited there is a refused connection.
    static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    pub(crate) async fn turn() -> tokio::sync::MutexGuard<'static, ()> {
        TURN.lock().await
    }

    /// A portrait as the API serves one: a JPEG — which is what an avatar URL hands back —
    /// decoded into the circle the topbar draws. The gradient is steep in both directions so
    /// the cells it paints cannot be mistaken for text.
    pub(crate) fn portrait(picker: &Picker) -> StatefulProtocol {
        let image = RgbImage::from_fn(64, 64, |x, y| Rgb([(x * 4) as u8, (y * 4) as u8, 60]));
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut bytes)
            .encode_image(&image)
            .expect("the fixture encodes");
        decode_circle(&bytes, picker).expect("and decodes")
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{fixtures::turn, *};

    /// A picker for a terminal without a graphics protocol: halfblocks is what every terminal
    /// can draw, so a portrait is never invisible to the tests.
    fn picker() -> Picker {
        Picker::halfblocks()
    }

    /// The cut is the shape of the avatar: nothing outside the inscribed circle survives it,
    /// whatever the picture had there, and the middle is the picture untouched.
    #[test]
    fn the_mask_keeps_the_centre_and_clears_the_corners() {
        let mut image = RgbaImage::from_pixel(48, 48, Rgba([200, 40, 20, 255]));
        mask_circle(&mut image);

        for (x, y) in [(0, 0), (47, 0), (0, 47), (47, 47)] {
            assert_eq!(image.get_pixel(x, y).0, [0, 0, 0, 0], "corner ({x},{y})");
        }
        assert_eq!(
            image.get_pixel(24, 24).0,
            [200, 40, 20, 255],
            "the middle is the face"
        );
        assert_eq!(
            image.get_pixel(0, 24).0[3],
            255,
            "the disc reaches its left edge"
        );
        assert_eq!(
            image.get_pixel(2, 2).0[3],
            0,
            "and stops well short of the corner"
        );
    }

    /// Bytes that are not an image are not a special case: they decode to nothing, which is
    /// the same answer as a failed download.
    #[test]
    fn bytes_that_are_not_an_image_decode_to_nothing() {
        assert!(decode_circle(b"not an image", &picker()).is_none());
    }

    /// Logging out forgets the portrait, and a load that was already on its way when it
    /// happened lands in the empty slot instead of putting the previous session's face back
    /// on the bar.
    #[tokio::test]
    async fn a_load_from_a_finished_session_is_dropped() {
        let _turn = turn().await;
        let session = session();
        let in_flight = fixtures::portrait(&picker());

        clear();

        assert!(
            !install(session, in_flight),
            "a finished session must not be served"
        );
        assert!(portrait().is_none());
    }

    /// A URL that cannot answer is not a special case either: the load reports nothing and
    /// nothing is installed, so the bar goes on drawing what it drew. Port 9 is the discard
    /// port, nothing listens on it, and nothing is listening on an empty URL either.
    #[tokio::test]
    async fn a_dead_url_installs_no_portrait() {
        let _turn = turn().await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let picker = picker();
        let http = Client::builder()
            .connect_timeout(Duration::from_millis(250))
            .build()
            .expect("client");

        clear();
        let session = session();

        for url in ["http://127.0.0.1:9/avatar.jpg", ""] {
            assert!(
                !fetch_and_install(&http, url, &picker, session).await,
                "{url:?} must not report a portrait"
            );
            assert!(portrait().is_none(), "{url:?} must not leave one behind");
        }
    }
}
