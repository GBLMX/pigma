use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use image::{DynamicImage, Rgba, RgbaImage};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
};

/// The placeholder glyphs, one per quarter turn.
const SPIN_GLYPHS: [char; 4] = ['\u{25F4}', '\u{25F5}', '\u{25F6}', '\u{25F7}'];

/// The cover placeholder's glyph when the spin is off.
const IDLE_GLYPH: char = '\u{266a}'; // ♪

/// Needle resting on the disc while it turns, and lifted while it is parked.
const NEEDLE_DOWN: char = '\u{2572}'; // ╲
const NEEDLE_UP: char = '\u{2502}'; // │

/// Cover image state shared with async loaders. `protocol` holds the decoded
/// image; `song_id` records which song the cover belongs to so a slow loader
/// from a previously played song can't overwrite the current cover.
///
/// It also holds the disc's rotation: `square` is the cover as decoded (square,
/// un-rotated RGBA) and `spin` is how far it has turned. Every angle is encoded
/// from that square rather than from the previous frame, so the sixteenth angle
/// of a turn is as sharp as the first instead of being sixteen resamples old.
pub struct CoverState {
    pub protocol: Arc<Mutex<Option<StatefulProtocol>>>,
    pub song_id: Arc<Mutex<Option<u64>>>,
    /// The decoded cover. Kept so an angle can be re-encoded without decoding the
    /// image (or downloading it) again.
    square: Arc<Mutex<Option<RgbaImage>>>,
    spin: Arc<Mutex<Spin>>,
}

/// How far the disc has turned, and what the last frame asked for.
#[derive(Debug, Clone, Default)]
struct Spin {
    /// `[playerbar] spinning_cover` as of the last frame.
    on: bool,
    /// The terminal draws real images. A halfblocks terminal draws the cover as
    /// coarse blocks, where a 5° turn shows nothing: there the spin becomes the
    /// placeholder glyph instead of a re-encoded image.
    graphics: bool,
    /// The disc is being listened to. A paused or stopped player freezes the disc
    /// where it stands.
    playing: bool,
    /// Angle of the disc, in turns (`0.0..1.0`).
    angle: f64,
    /// The angle step `angle` falls in, and the step `protocol` was encoded at.
    /// They differ only between a step being reached and the re-encode it
    /// triggers; while they are equal the frame draws the protocol already in
    /// hand, which costs nothing — a terminal only re-transmits an image it has
    /// never been sent.
    step: u32,
    encoded: u32,
    /// When the angle last advanced. `None` while parked, so a resume does not
    /// charge the pause to the disc.
    last: Option<Instant>,
}

impl CoverState {
    /// Angle steps per turn: 5° apart, so a turn re-encodes every 278 ms.
    pub const STEPS_PER_TURN: u32 = 72;

    /// Seconds per turn. Slow on purpose: the cover is ambient art while a track
    /// plays, and a record's own 33⅓ rpm (1.8 s a turn) reads as a spinner.
    pub const TURN_SECS: f64 = 20.0;

    /// Turn `square` about its centre by `turns` (a fraction of a full turn, any
    /// sign, one turn per unit) into an image of the same size, bilinearly sampled.
    ///
    /// A rotated square does not fit a square: the corners leave the frame and read
    /// as transparent. That is where the round mask cuts anyway, so nothing the
    /// player bar draws is lost.
    fn rotate(square: &RgbaImage, turns: f32) -> RgbaImage {
        let (width, height) = square.dimensions();
        let (sin, cos) = (turns * std::f32::consts::TAU).sin_cos();
        let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);

        let mut turned = RgbaImage::new(width, height);
        for (x, y, pixel) in turned.enumerate_pixels_mut() {
            // Walk backwards — for this destination pixel, the source pixel it came
            // from. Sampling the other way round would leave gaps.
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let (sx, sy) = (
                cos * dx + sin * dy + cx - 0.5,
                -sin * dx + cos * dy + cy - 0.5,
            );
            *pixel = sample(square, sx, sy);
        }
        turned
    }

    /// Cut `image` down to the disc: everything outside the inscribed circle turns
    /// transparent. Applied *after* the rotation, so the circle's edge is re-cut at
    /// the new angle instead of being resampled — which is what keeps it sharp.
    fn mask(image: &mut RgbaImage) {
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

    /// The protocol the player bar draws for `square` turned by `turns`: rotate,
    /// mask round, then hand to `picker` to encode.
    ///
    /// Rotating before masking is what keeps the disc's edge sharp: the round cut is
    /// made at the new angle instead of being resampled along with the picture, so the
    /// rim cannot soften step by step.
    pub fn encode_disc(square: &RgbaImage, turns: f32, picker: &Picker) -> StatefulProtocol {
        // A cover arrives at angle zero, and a zero turn is not a rotation: copying the
        // square beats resampling it (3 µs against 830 µs for a 200×200 cover, measured
        // by `cover_bench`).
        let mut image = if turns == 0.0 {
            square.clone()
        } else {
            Self::rotate(square, turns)
        };
        Self::mask(&mut image);
        picker.new_resize_protocol(DynamicImage::ImageRgba8(image))
    }

    /// Install a cover that just finished loading: the protocol to draw and the
    /// square to re-encode from. Dropped when the song changed while it was
    /// loading — a stale loader must not overwrite a newer cover.
    pub fn install(&self, song_id: u64, square: RgbaImage, protocol: StatefulProtocol) {
        let still_current = self
            .song_id
            .lock()
            .map(|g| *g == Some(song_id))
            .unwrap_or(false);
        if !still_current {
            return;
        }
        if let Ok(mut guard) = self.square.lock() {
            *guard = Some(square);
        }
        self.rewind();
        if let Ok(mut guard) = self.protocol.lock() {
            *guard = Some(protocol);
        }
    }

    /// Forget the cover: a new song must not show the previous one's art, and its
    /// own cover starts at the top of its turn.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.protocol.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.square.lock() {
            *guard = None;
        }
        self.rewind();
    }

    /// Back to angle zero, with the clock stopped.
    fn rewind(&self) {
        if let Ok(mut spin) = self.spin.lock() {
            spin.angle = 0.0;
            spin.step = 0;
            spin.encoded = 0;
            spin.last = None;
        }
    }

    /// Advance the spin to now, re-encoding the cover once the disc reached its
    /// next angle step. Meant to be called once a frame: an angle that did not
    /// change encodes nothing, so the spin never turns a render that costs nothing
    /// into an encode and a transmit every frame.
    pub fn advance(&self, picker: &Picker, on: bool, playing: bool) {
        self.advance_at(Instant::now(), picker, on, playing);
    }

    /// [`CoverState::advance`] against a given clock, so tests can turn the disc.
    pub(crate) fn advance_at(&self, now: Instant, picker: &Picker, on: bool, playing: bool) {
        let Ok(mut spin) = self.spin.lock() else {
            return;
        };
        spin.on = on;
        spin.graphics = picker.protocol_type() != ProtocolType::Halfblocks;
        spin.playing = playing;

        if !(on && playing) {
            // Parked. The pause is not part of the disc's angle: stop the clock, so
            // playing again resumes from the angle it stopped at instead of jumping
            // ahead by however long the pause lasted.
            spin.last = None;
            return;
        }
        let Some(previous) = spin.last.replace(now) else {
            // First frame of a turn: start the clock, there is nothing to add yet.
            return;
        };
        spin.angle = (spin.angle + now.duration_since(previous).as_secs_f64() / Self::TURN_SECS)
            .rem_euclid(1.0);
        spin.step = (spin.angle * f64::from(Self::STEPS_PER_TURN)) as u32;
        if spin.step == spin.encoded || !spin.graphics {
            return;
        }

        // Encode from the square while its lock is held: nothing else writes it (only
        // a cover load does), and the resulting image is passed on, not borrowed.
        let protocol = {
            let Ok(square) = self.square.lock() else {
                return;
            };
            let Some(square) = square.as_ref() else {
                return;
            };
            let turn = spin.step as f32 / Self::STEPS_PER_TURN as f32;
            Self::encode_disc(square, turn, picker)
        };
        if let Ok(mut guard) = self.protocol.lock() {
            *guard = Some(protocol);
        }
        spin.encoded = spin.step;
    }

    /// Whether the disc is shown by turning the placeholder glyph instead of the
    /// image: the spin is on, but the terminal has no graphics protocol, so the
    /// cover is drawn as coarse half-blocks where the angle would not read.
    pub fn spin_as_glyph(&self) -> bool {
        self.spin.lock().is_ok_and(|spin| spin.on && !spin.graphics)
    }

    /// The cover placeholder's centre glyph: `♪`, or the disc's angle as one of the
    /// four quarter-turn glyphs while the spin is on.
    pub fn spin_glyph(&self) -> char {
        let Ok(spin) = self.spin.lock() else {
            return IDLE_GLYPH;
        };
        if !spin.on {
            return IDLE_GLYPH;
        }
        let quadrant = spin.step as usize * SPIN_GLYPHS.len() / Self::STEPS_PER_TURN as usize;
        SPIN_GLYPHS[quadrant.min(SPIN_GLYPHS.len() - 1)]
    }

    /// The needle beside the disc — on it while the cover turns, lifted while it is
    /// parked — or `None` while the spin is off and the placeholder is a plain `♪`.
    pub fn tonearm(&self) -> Option<char> {
        let spin = self.spin.lock().ok()?;
        spin.on
            .then_some(if spin.playing { NEEDLE_DOWN } else { NEEDLE_UP })
    }
}

/// Bilinear sample of `src` at pixel coordinate `(x, y)`; outside the image reads as
/// transparent.
///
/// Colours are weighted by their own alpha and divided by the alpha actually mixed,
/// so a transparent neighbour cannot darken the edge it sits next to. The disc never
/// samples outside the image (a rotation maps the inscribed circle onto itself), but
/// the corners do — and covers with an alpha channel exist.
fn sample(src: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let (width, height) = src.dimensions();
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as i32, y0 as i32);

    // Colours premultiplied by their coverage, and the coverage itself: dividing by
    // what was actually mixed (rather than by the sum of the weights) is what keeps a
    // fully transparent neighbour from pulling the edge towards black.
    let mut mixed = [0.0f32; 3];
    let mut coverage = 0.0f32;
    for (ix, iy, weight) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1, y0, fx * (1.0 - fy)),
        (x0, y0 + 1, (1.0 - fx) * fy),
        (x0 + 1, y0 + 1, fx * fy),
    ] {
        if weight == 0.0 || ix < 0 || iy < 0 || ix >= width as i32 || iy >= height as i32 {
            continue;
        }
        let neighbour = src.get_pixel(ix as u32, iy as u32).0;
        let covered = f32::from(neighbour[3]) / 255.0 * weight;
        coverage += covered;
        for (channel, value) in mixed.iter_mut().zip(&neighbour[..3]) {
            *channel += f32::from(*value) * covered;
        }
    }

    if coverage <= 0.0 {
        return Rgba([0, 0, 0, 0]);
    }
    Rgba([
        (mixed[0] / coverage).round() as u8,
        (mixed[1] / coverage).round() as u8,
        (mixed[2] / coverage).round() as u8,
        (coverage.min(1.0) * 255.0).round() as u8,
    ])
}

impl std::fmt::Debug for CoverState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverState")
            .field(
                "has_cover",
                &self.protocol.lock().map(|g| g.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

impl Clone for CoverState {
    fn clone(&self) -> Self {
        Self {
            protocol: Arc::clone(&self.protocol),
            song_id: Arc::clone(&self.song_id),
            square: Arc::clone(&self.square),
            spin: Arc::clone(&self.spin),
        }
    }
}

impl Default for CoverState {
    fn default() -> Self {
        Self {
            protocol: Arc::new(Mutex::new(None)),
            song_id: Arc::new(Mutex::new(None)),
            square: Arc::new(Mutex::new(None)),
            spin: Arc::new(Mutex::new(Spin::default())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
    use ratatui_image::{Resize, StatefulImage};

    use super::*;

    /// The cover area the modern layout gives the disc.
    const COVER: Rect = Rect {
        x: 0,
        y: 0,
        width: 8,
        height: 3,
    };

    /// A square with something in every corner and a gradient across it, so a
    /// rotation has to move all of it.
    fn test_square(size: u32) -> RgbaImage {
        RgbaImage::from_fn(size, size, |x, y| {
            Rgba([(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255])
        })
    }

    /// A picker for the graphics path: a terminal without graphics protocol never
    /// reaches the re-encode (`spin_as_glyph` sends it to the glyph instead).
    fn graphics_picker() -> Picker {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        picker
    }

    /// A cover installed for song 7, mid-turn state included.
    fn installed_cover(picker: &Picker) -> CoverState {
        let cover = CoverState::default();
        *cover.song_id.lock().expect("song id") = Some(7);
        let square = test_square(24);
        let protocol = CoverState::encode_disc(&square, 0.0, picker);
        cover.install(7, square, protocol);
        cover
    }

    /// What the frame would actually put on the screen for the disc. A kitty
    /// protocol carries its image in the first cell it draws, so that cell is the
    /// whole picture for the purpose of "was this re-encoded".
    fn rendered(cover: &CoverState) -> String {
        let mut guard = cover.protocol.lock().expect("protocol");
        let protocol = guard.as_mut().expect("a cover");
        let mut buffer = Buffer::empty(COVER);
        StatefulImage::new()
            .resize(Resize::Fit(None))
            .render(COVER, &mut buffer, protocol);
        let cell = buffer.cell((0, 0)).expect("cell");
        format!("{:?}{:?}", cell.symbol(), cell.fg)
    }

    /// Four quarter turns are no turn at all: bilinear sampling must not accumulate
    /// a visible shift, which is what rotating an already-rotated frame would do.
    #[test]
    fn a_quarter_turn_four_times_is_the_original() {
        let square = test_square(24);
        let mut turned = square.clone();
        for _ in 0..4 {
            turned = CoverState::rotate(&turned, 0.25);
        }

        for (at, (original, turned)) in square.pixels().zip(turned.pixels()).enumerate() {
            for channel in 0..4 {
                let (before, after) = (original.0[channel], turned.0[channel]);
                assert!(
                    before.abs_diff(after) <= 2,
                    "pixel {at} channel {channel}: {before} became {after}"
                );
            }
        }
    }

    /// The round edge is cut after the rotation, so the corners of the frame are
    /// still fully transparent — the circle is what is drawn, not the square.
    #[test]
    fn the_rotated_disc_keeps_its_corners_transparent() {
        let square = test_square(24);
        let mut disc = CoverState::rotate(&square, 0.19);
        CoverState::mask(&mut disc);

        for (x, y) in [(0, 0), (23, 0), (0, 23), (23, 23)] {
            assert_eq!(disc.get_pixel(x, y).0, [0, 0, 0, 0], "corner ({x},{y})");
        }
        assert_eq!(disc.get_pixel(12, 12).0[3], 255, "the middle is the cover");
        assert_eq!(
            disc.get_pixel(0, 12).0[3],
            255,
            "the disc reaches its left edge"
        );
    }

    /// The angle advances with wall-clock time (not with a frame count) and only
    /// while the player is playing: a pause freezes the disc, and playing again
    /// resumes from that angle rather than jumping by the length of the pause.
    #[test]
    fn the_disc_only_turns_while_it_plays() {
        let picker = graphics_picker();
        let cover = installed_cover(&picker);
        let start = Instant::now();
        // A quarter turn is 5 s of a 20 s turn: enough to move the placeholder glyph
        // a quadrant, whatever the step arithmetic does in between.
        let quarter = |secs| start + Duration::from_secs(secs);

        cover.advance_at(start, &picker, true, true);
        cover.advance_at(quarter(5), &picker, true, true);
        assert_eq!(cover.spin_glyph(), SPIN_GLYPHS[1], "a quarter turn on");

        cover.advance_at(quarter(60), &picker, true, false);
        cover.advance_at(quarter(90), &picker, true, false);
        assert_eq!(
            cover.spin_glyph(),
            SPIN_GLYPHS[1],
            "a pause must not turn it"
        );

        cover.advance_at(quarter(91), &picker, true, true);
        assert_eq!(
            cover.spin_glyph(),
            SPIN_GLYPHS[1],
            "and must not jump on resume"
        );
        cover.advance_at(quarter(96), &picker, true, true);
        assert_eq!(cover.spin_glyph(), SPIN_GLYPHS[2], "half a turn on");

        cover.advance_at(quarter(200), &picker, false, true);
        cover.advance_at(quarter(300), &picker, false, true);
        assert_eq!(cover.spin_glyph(), IDLE_GLYPH, "off is a plain ♪ again");
    }

    /// The cover is re-encoded when the disc reaches a new angle step and not once
    /// per frame. A still frame must leave the protocol object in place: re-encoding
    /// per frame would replace the image the terminal already holds, turning a render
    /// that costs nothing into an encode and a transmit every frame.
    #[test]
    fn only_a_new_angle_re_encodes_the_disc() {
        let picker = graphics_picker();
        let cover = installed_cover(&picker);
        let start = Instant::now();
        let step_ms = CoverState::TURN_SECS * 1000.0 / f64::from(CoverState::STEPS_PER_TURN);
        let step = Duration::from_micros((step_ms * 1000.0) as u64);

        cover.advance_at(start, &picker, true, true);
        // Twice: the first render of a protocol is the one that transmits the image,
        // the ones after it only place it. What has to stay equal is the steady state.
        rendered(&cover);
        let first = rendered(&cover);
        // Mid-step: the disc has moved, but not to a new angle, so the protocol must be
        // the one already in hand — image and id alike.
        cover.advance_at(start + step / 2, &picker, true, true);
        assert_eq!(rendered(&cover), first, "a still disc encodes nothing");
        // The next step does encode, so the line above is not one that can never fail.
        cover.advance_at(
            start + step + Duration::from_millis(10),
            &picker,
            true,
            true,
        );
        assert_ne!(rendered(&cover), first, "a new angle has to be encoded");
    }

    /// A halfblocks terminal has no image to turn: the spin is the placeholder glyph
    /// and the needle, and nothing re-encodes the cover behind them.
    #[test]
    fn a_halfblocks_terminal_turns_the_placeholder_instead() {
        let picker = Picker::halfblocks();
        let cover = installed_cover(&picker);
        let start = Instant::now();
        let before = rendered(&cover);

        cover.advance_at(start, &picker, true, true);
        cover.advance_at(start + Duration::from_secs(5), &picker, true, true);
        assert!(cover.spin_as_glyph());
        assert_eq!(cover.spin_glyph(), SPIN_GLYPHS[1]);
        assert_eq!(rendered(&cover), before, "the image is left as it was");
    }

    /// The needle is the spin's second half: down while the cover turns, up while it
    /// is parked, and absent while the spin is off.
    #[test]
    fn the_needle_follows_the_playing_state() {
        let picker = graphics_picker();
        let cover = CoverState::default();
        assert_eq!(cover.tonearm(), None, "off is a plain ♪ placeholder");

        cover.advance_at(Instant::now(), &picker, true, true);
        assert_eq!(cover.tonearm(), Some(NEEDLE_DOWN));
        cover.advance_at(Instant::now(), &picker, true, false);
        assert_eq!(cover.tonearm(), Some(NEEDLE_UP));
    }

    /// A cover that arrives after the song changed is dropped, square and all, so a
    /// slow loader cannot leave a later song spinning the wrong disc.
    #[test]
    fn a_stale_cover_is_dropped_with_its_square() {
        let picker = graphics_picker();
        let cover = installed_cover(&picker);
        let square = test_square(24);
        let protocol = CoverState::encode_disc(&square, 0.0, &picker);

        assert_eq!(size_of(&cover), Some((24, 24)));
        cover.install(8, square, protocol);
        assert_eq!(
            size_of(&cover),
            Some((24, 24)),
            "song 8 never asked for a cover"
        );

        cover.clear();
        assert_eq!(size_of(&cover), None);
        assert_eq!(cover.spin_glyph(), IDLE_GLYPH);
    }

    fn size_of(cover: &CoverState) -> Option<(u32, u32)> {
        cover
            .square
            .lock()
            .ok()
            .and_then(|square| square.as_ref().map(image::GenericImageView::dimensions))
    }
}
