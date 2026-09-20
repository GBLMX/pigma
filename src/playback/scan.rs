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
    let properties = tagged.properties().duration();
    LocalTags {
        title: text(ItemKey::TrackTitle),
        // Same precedence the cloud upload uses, so a track's `singer` matches
        // what gets uploaded for it.
        artist: text(ItemKey::TrackArtist).or_else(|| text(ItemKey::AlbumArtist)),
        album: text(ItemKey::AlbumTitle),
        duration: (!properties.is_zero()).then_some(properties.as_millis() as u64),
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
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut songs = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            songs.extend(scan_local_music(&path));
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
        // Clear the top bit so the id can never collide with the sonar song-id
        // flag (1<<63); otherwise ~half of local files get misrouted to the
        // network resolver instead of `resolve_local`.
        let id = hasher.finish() & !(1u64 << 63);
        songs.push(SongInfo {
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
        });
    }
    songs.sort_by(|a, b| a.name.cmp(&b.name));
    songs
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
        // The top bit marks sonar ids; a local id carrying it would be routed to
        // the network resolver instead of the file on disk.
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

    fn fixture_library(count: usize) -> PathBuf {
        let dir = std::env::temp_dir().join("boxpigma-scan-bench");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create fixture dir");
        let sources: Vec<PathBuf> = [
            "/tmp/localmusic/带标签的歌.flac",
            "/tmp/localmusic/无标签的歌.mp3",
        ]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect();
        assert!(!sources.is_empty(), "需要 /tmp/localmusic 下的素材");
        for i in 0..count {
            let src = &sources[i % sources.len()];
            let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let dst = dir.join(format!("track-{i:05}.{ext}"));
            if fs::hard_link(src, &dst).is_err() {
                fs::copy(src, &dst).expect("copy fallback");
            }
        }
        dir
    }

    #[test]
    #[ignore]
    fn scanning_a_library_costs() {
        let dir = fixture_library(300);
        let songs = super::scan_local_music(&dir);
        assert!(!songs.is_empty(), "扫描应当找到文件");
        println!("  素材: {} 首（其中带标签/无标签交替）", songs.len());

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
