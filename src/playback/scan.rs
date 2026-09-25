use std::{
    fs,
    hash::{Hash, Hasher},
    io::BufReader,
    path::Path,
};

use ncm_api::SongInfo;
use rodio::Source;

/// Extensions indexed as local audio (what the decoder behind the app can open).
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "aac", "m4a", "wma"];

/// What a file's tags had to say about a track.
///
/// Every field is optional on purpose: a file with no tags at all — or one
/// `lofty` cannot parse — still has to produce a usable entry, so the scan
/// keeps the filename/decoder values it used before.
#[derive(Debug, Default)]
struct LocalTags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    /// Milliseconds; read off the container header, without decoding a sample.
    duration: Option<u64>,
    /// Position in the album, as the tags carry it. Only used to order the
    /// scan's output (see [`TrackPosition`]); nothing displays it.
    disc: Option<u32>,
    track: Option<u32>,
}

/// Read title/artist/album/duration from a file's tags.
///
/// Cover art is switched off deliberately: nothing here consumes it, and it is
/// the one tag value that costs real memory to materialize (an embedded JPEG
/// can be megabytes), which a library-wide scan would pay for per file.
fn read_tags(path: &Path) -> LocalTags {
    use lofty::{
        config::ParseOptions,
        file::{AudioFile, TaggedFileExt},
        probe::Probe,
        tag::ItemKey,
    };

    let Ok(tagged) = Probe::open(path).and_then(|probe| {
        probe
            .options(ParseOptions::new().read_cover_art(false))
            .read()
    }) else {
        return LocalTags::default();
    };
    // Primary tag first (ID3v2 for mp3, Vorbis comments for flac, ...), then
    // whatever else the file carries: a file tagged by a different tool may
    // only have the secondary one.
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    // A blank frame is as good as a missing one: an ID3 field holding spaces
    // must not displace the filename fallback.
    let text = |key: ItemKey| -> Option<String> {
        tag.and_then(|t| t.get_string(key))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    // Track/disc numbers are text in every tag format this reads, and taggers
    // pad and decorate them freely ("03", "3/13", "1 of 2"), so the number is
    // taken up to the first separator and anything unparsable stays `None`
    // rather than throwing the whole position away.
    let number = |key: ItemKey| -> Option<u32> {
        text(key)?
            .split(['/', ' '])
            .next()?
            .trim()
            .parse::<u32>()
            .ok()
    };
    let properties = tagged.properties().duration();
    LocalTags {
        title: text(ItemKey::TrackTitle),
        // Same precedence the cloud upload uses, so a track's `singer` matches
        // what gets uploaded for it.
        artist: text(ItemKey::TrackArtist).or_else(|| text(ItemKey::AlbumArtist)),
        album: text(ItemKey::AlbumTitle),
        duration: (!properties.is_zero()).then_some(properties.as_millis() as u64),
        disc: number(ItemKey::DiscNumber),
        track: number(ItemKey::TrackNumber),
    }
}

/// Fallback duration: decode the file to let `rodio` report its total length.
///
/// Only reached when the tags carry no usable length (no tags at all, or a
/// container whose properties could not be read). `0` means "unknown", as before.
fn decoded_duration(path: &Path) -> u64 {
    fs::File::open(path)
        .ok()
        .and_then(|f| rodio::Decoder::new(BufReader::new(f)).ok())
        .and_then(|d| d.total_duration())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn scan_local_music(dir: &std::path::Path) -> Vec<SongInfo> {
    let mut songs = Vec::new();
    scan_local_dir(dir, &mut songs);
    // Album order: the album a file belongs to, then its position in it, then the
    // path. Sorting on the title alone (`a.name.cmp(&b.name)`) is what played a CD
    // rip out of order — `Airport Arrival` came before `Airport Take Off`, and the
    // rest followed whatever their titles collated to. The path is the tiebreak for
    // a library with no track tags at all, where it is at least the same order on
    // every machine; `read_dir` order, which used to leak into the queue, is not.
    songs.sort_by(|a, b| {
        a.1.album
            .cmp(&b.1.album)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.local_path.cmp(&b.1.local_path))
    });
    songs.into_iter().map(|(_, song)| song).collect()
}

/// Disc and track number of one file, both `None` when its tags carry neither.
///
/// `Option`'s `Ord` compares `None` below `Some`, so untagged files of an album
/// sort ahead of its numbered ones. Arbitrary, but the same everywhere, which is
/// the whole point of ordering here.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TrackPosition {
    disc: Option<u32>,
    track: Option<u32>,
}

/// Walk `dir`, appending every audio file it holds (and the subtrees under it)
/// together with the position that orders it. The traversal order is deliberately
/// meaningless — [`scan_local_music`] sorts what it collects.
fn scan_local_dir(dir: &Path, songs: &mut Vec<(TrackPosition, SongInfo)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            scan_local_dir(&path, songs);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let tags = read_tags(&path);
        let name = tags.title.unwrap_or_else(|| {
            path.file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });
        let duration = tags.duration.unwrap_or_else(|| decoded_duration(&path));
        let path_text = path.to_string_lossy().into_owned();
        // The album tag wins; without one the path stays what the album column
        // showed before tags were read at all.
        let album = tags.album.unwrap_or_else(|| path_text.clone());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        // The top bit is cleared to keep the ids stable: it used to separate these local ids
        // from the (now removed) network-source ids, and the masked values are already written
        // into the on-disk queue files. Dropping the mask would change the id of roughly half of
        // all local files and orphan their saved queues.
        let id = hasher.finish() & !(1u64 << 63);
        songs.push((
            TrackPosition {
                disc: tags.disc,
                track: tags.track,
            },
            SongInfo {
                id,
                name,
                singer: tags.artist.unwrap_or_else(|| "本地".into()),
                artist_id: 0,
                album,
                album_id: 0,
                pic_url: String::new(),
                duration,
                mv: 0,
                copyright: ncm_api::SongCopyright::Free,
                // The album field may now hold the album tag, so the file the track
                // plays from travels in its own field (see `playback::source`).
                local_path: Some(path_text),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lofty::{
        config::WriteOptions,
        tag::{ItemKey, Tag, TagExt},
    };

    use super::*;

    /// A scratch directory this test owns; `label` keeps parallel tests apart.
    fn temp_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("boxpigma-scan-test-{}-{label}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Write a 16-bit mono PCM WAV holding `samples` samples of silence.
    ///
    /// A synthesized file beats a checked-in fixture: it is a couple of hundred
    /// bytes of header math, `lofty` can tag it and `rodio` can decode it, and
    /// the exact duration is known (`samples / rate` seconds) without a binary
    /// blob nobody can review.
    fn write_wav(path: &Path, samples: u32) {
        const RATE: u32 = 8000;
        const CHANNELS: u16 = 1;
        const BITS: u16 = 16;
        let data_len = samples * u32::from(CHANNELS) * u32::from(BITS / 8);
        let byte_rate = RATE * u32::from(CHANNELS) * u32::from(BITS / 8);
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&CHANNELS.to_le_bytes());
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
        wav.extend_from_slice(&BITS.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(wav.len() + data_len as usize, 0);
        fs::write(path, wav).expect("write wav");
    }

    /// Tag a file through the public `lofty` writer, so the scan reads what a
    /// real tag editor would have left behind.
    fn write_tags(path: &Path, items: &[(ItemKey, &str)]) {
        let mut tag = Tag::new(lofty::file::FileType::Wav.primary_tag_type());
        for (key, value) in items {
            assert!(tag.insert_text(*key, (*value).to_string()));
        }
        tag.save_to_path(path, WriteOptions::default())
            .expect("write tags");
    }

    /// 8000 samples at 8 kHz: the duration has to come back as exactly 1000 ms.
    const SAMPLES: u32 = 8000;

    #[test]
    fn tags_supply_the_title_artist_album_and_duration() {
        let dir = temp_dir("tagged");
        let audio = dir.join("file-stem.wav");
        write_wav(&audio, SAMPLES);
        write_tags(
            &audio,
            &[
                (ItemKey::TrackTitle, "标签标题"),
                (ItemKey::TrackArtist, "标签歌手"),
                (ItemKey::AlbumTitle, "标签专辑"),
            ],
        );

        let songs = scan_local_music(&dir);
        assert_eq!(songs.len(), 1);
        let song = &songs[0];
        assert_eq!(song.name, "标签标题");
        assert_eq!(song.singer, "标签歌手");
        assert_eq!(song.album, "标签专辑");
        assert_eq!(song.duration, 1000);
        assert_eq!(
            song.local_path.as_deref(),
            Some(audio.to_string_lossy().as_ref()),
            "the player needs the real file, not the album tag"
        );
        // The top bit is kept clear for id stability across versions; a local id carrying it
        // would no longer match the ids already saved in the queue files.
        assert_eq!(song.id & (1 << 63), 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_without_tags_keeps_its_filename_and_path() {
        let dir = temp_dir("untagged");
        let plain = dir.join("Artist - Song.wav");
        write_wav(&plain, SAMPLES);
        // Only some fields tagged: the missing ones fall back on their own.
        let partial = dir.join("Partial.wav");
        write_wav(&partial, SAMPLES);
        write_tags(
            &partial,
            &[
                (ItemKey::TrackTitle, "有标题"),
                (ItemKey::TrackArtist, "有歌手"),
            ],
        );

        let songs = scan_local_music(&dir);
        let by_name = |name: &str| {
            songs
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("no song named {name} in {songs:?}"))
        };

        let untagged = by_name("Artist - Song");
        assert_eq!(untagged.singer, "本地");
        assert_eq!(untagged.album, plain.to_string_lossy().as_ref());
        // No tag properties to read, so this one came from the decoder.
        assert_eq!(untagged.duration, 1000);

        let partial_song = by_name("有标题");
        assert_eq!(partial_song.singer, "有歌手");
        assert_eq!(partial_song.album, partial.to_string_lossy().as_ref());
        assert_eq!(partial_song.duration, 1000);

        fs::remove_dir_all(&dir).ok();
    }

    /// A rip's tags say in what order its tracks play; the scan sorted on the title
    /// instead, which played an album in whatever order its titles collated to
    /// (`Airport Arrival` before `Airport Take Off`). Both the directory order and a
    /// title sort fail this: the files are written out of order and their titles sort
    /// the opposite way from their track numbers.
    #[test]
    fn tracks_follow_their_album_position_not_their_title() {
        let dir = temp_dir("track-order");
        let write = |file: &str, disc: &str, track: &str, title: &str| {
            let path = dir.join(file);
            write_wav(&path, SAMPLES);
            write_tags(
                &path,
                &[
                    (ItemKey::AlbumTitle, "专辑"),
                    (ItemKey::DiscNumber, disc),
                    (ItemKey::TrackNumber, track),
                    (ItemKey::TrackTitle, title),
                ],
            );
        };
        write("03 - zzz.wav", "1", "3", "Zzz");
        write("01 - mmm.wav", "1", "1", "Mmm");
        write("02 - aaa.wav", "1", "2", "Aaa");

        let names: Vec<String> = scan_local_music(&dir)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, ["Mmm", "Aaa", "Zzz"]);

        fs::remove_dir_all(&dir).ok();
    }

    /// A multi-disc album lives in one folder, so the disc number has to come before
    /// the track number. Taggers write these as `1/13`, which is why the number is
    /// taken up to the separator.
    #[test]
    fn discs_play_before_tracks_and_totals_do_not_confuse_the_number() {
        let dir = temp_dir("disc-order");
        let write = |file: &str, disc: &str, track: &str, title: &str| {
            let path = dir.join(file);
            write_wav(&path, SAMPLES);
            write_tags(
                &path,
                &[
                    (ItemKey::AlbumTitle, "双碟"),
                    (ItemKey::DiscNumber, disc),
                    (ItemKey::TrackNumber, track),
                    (ItemKey::TrackTitle, title),
                ],
            );
        };
        write("disc-2-track-1.wav", "2/2", "1/9", "第二碟第一首");
        write("disc-1-track-2.wav", "1/2", "2/9", "第一碟第二首");
        write("disc-1-track-1.wav", "1/2", "1/9", "第一碟第一首");

        let names: Vec<String> = scan_local_music(&dir)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, ["第一碟第一首", "第一碟第二首", "第二碟第一首"]);

        fs::remove_dir_all(&dir).ok();
    }

    /// A library with no track tags anywhere still has to come out in the same order
    /// on every machine: the path decides, not the filesystem. `read_dir` order used
    /// to reach the queue unchanged, which is why a scan of the same directory could
    /// list the same songs differently from one run to the next.
    #[test]
    fn a_library_without_track_tags_is_ordered_by_its_paths() {
        let dir = temp_dir("untagged-order");
        for file in ["c - third.wav", "a - first.wav", "b - second.wav"] {
            write_wav(&dir.join(file), SAMPLES);
        }

        // Titles fall back on the file stems, so the names double as the paths' order.
        let names: Vec<String> = scan_local_music(&dir)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, ["a - first", "b - second", "c - third"]);

        fs::remove_dir_all(&dir).ok();
    }
}

/// `cargo test --release --lib -- --ignored --nocapture scan_bench`
///
/// What reading a music library costs. The scan is serial and parses every file's tags, so a
/// few thousand tracks are the difference between "instant" and "wait for it" — this is the
/// number that decides whether that is worth changing. The fixture is a farm of hard links to
/// the test fixtures, so the files themselves stay in the page cache and the measurement is the
/// tag parsing, not the disk.
#[cfg(test)]
mod scan_bench {
    use std::{fs, path::PathBuf};

    /// Where the bench looks for its source audio: `BOXPIGMA_BENCH_MUSIC` when set, else the
    /// platform's music directory. The fixtures have to be real files because the point is
    /// tag parsing, and tag parsing needs tags.
    fn source_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("BOXPIGMA_BENCH_MUSIC") {
            return PathBuf::from(dir);
        }
        dirs::audio_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Music"))
    }

    /// Hard links `count` files out of [`source_dir`], or `None` when there is no audio to
    /// link — a bench that fails on every machine but the author's is a bench nobody runs.
    fn fixture_library(count: usize) -> Option<PathBuf> {
        let sources: Vec<PathBuf> = fs::read_dir(source_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("mp3" | "flac" | "m4a" | "wav" | "ogg" | "opus")
                    )
            })
            .collect();
        if sources.is_empty() {
            return None;
        }

        let dir = std::env::temp_dir().join("boxpigma-scan-bench");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create fixture dir");
        for i in 0..count {
            let src = &sources[i % sources.len()];
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let dst = dir.join(format!("track-{i:05}.{ext}"));
            if fs::hard_link(src, &dst).is_err() {
                fs::copy(src, &dst).expect("copy fallback");
            }
        }
        Some(dir)
    }

    #[test]
    #[ignore]
    fn scanning_a_library_costs() {
        let Some(dir) = fixture_library(300) else {
            println!(
                "  没有可用的素材，跳过：把音频放进 {} 或设置 BOXPIGMA_BENCH_MUSIC",
                source_dir().display()
            );
            return;
        };
        let songs = super::scan_local_music(&dir);
        assert!(!songs.is_empty(), "扫描应当找到文件");
        println!(
            "  素材: {} 首，来自 {}",
            songs.len(),
            source_dir().display()
        );

        let per_scan = crate::bench_util::time("扫描整个曲库（300 首）", 3, || {
            std::hint::black_box(super::scan_local_music(&dir));
        });
        // Seconds at this scale read as "0.0", which hides what this number is for: the scan is
        // cheap in CPU terms, so parallelising it would save tens of milliseconds, once.
        println!(
            "  → 折合每首 {:.2} µs；若按 5000 首估算约 {:.0} ms（当前串行）",
            per_scan * 1e6 / 300.0,
            per_scan * (5000.0 / 300.0) * 1000.0
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
