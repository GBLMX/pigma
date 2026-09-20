use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use image::GenericImageView;
use ncm_api::SongInfo;

use super::{App, send_event};
use crate::{
    event::{AppEvent, NavigationEvent, PlaybackEvent},
    playback::{CoverState, NCM_SEARCH_QUEUE_KEY, THIRD_PARTY_QUEUE_KEY, parse_lyric_lines},
    state::{ContentState, PaginationInfo, mv},
};

impl App {
    pub(super) fn handle_content_loaded(&mut self, content: ContentState) {
        self.state.navigation.set_content(content);
    }

    pub(super) fn handle_load_more(&mut self) {
        let (api, offset, limit) = match self.state.navigation.pagination.as_ref() {
            Some(pg) if pg.has_more => (pg.api.clone(), pg.next_offset(), pg.limit),
            _ => return,
        };

        let service = self.service.clone();
        let sender = self.state.events.sender();
        let generation = self.state.navigation.generation;

        tokio::spawn(async move {
            match service.load_more(&api, offset, limit).await {
                Some((content, pagination)) => send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content,
                        pagination,
                        generation,
                    }
                    .into(),
                ),
                // Release the in-flight flag, otherwise pagination stays stuck
                // after a single transient failure.
                None => send_event(&sender, NavigationEvent::LoadMoreFailed.into()),
            }
        });
    }

    pub(super) fn handle_content_loaded_paged(
        &mut self,
        content: ContentState,
        pagination: PaginationInfo,
        generation: u64,
    ) {
        // Drop stale responses
        if generation != 0 && generation != self.state.navigation.generation {
            return;
        }

        let same_api =
            self.state.navigation.pagination.as_ref().map(|p| &p.api) == Some(&pagination.api);

        let mut content = content;

        // Only song lists (cloud disk, songs within a playlist) support paged appends; other types replace the whole content.
        if same_api
            && let ContentState::Songs(new_songs) = &mut content
            && matches!(
                self.state.navigation.content.as_ref(),
                ContentState::Songs(_)
            )
        {
            std::sync::Arc::make_mut(&mut self.state.navigation.content)
                .append_unique_songs(std::mem::take(new_songs));
            let pg_for_save = pagination.clone();
            self.state.navigation.pagination = Some(pagination);

            let ttl = self.config.cache.content_cache_ttl;
            if ttl > 0 && !pg_for_save.api.is_empty() {
                let cache = self.service.cache().clone();
                let content_arc = Arc::clone(&self.state.navigation.content);
                tokio::task::spawn_blocking(move || {
                    cache.save_content_cache(&pg_for_save.api, &content_arc, Some(&pg_for_save));
                });
            }
            return;
        }
        self.state.navigation.set_content(content);
        self.state.navigation.pagination = Some(pagination);
    }

    pub(super) fn handle_playlist_select(&mut self, id: u64, name: Option<String>) {
        self.state.navigation.push_breadcrumb();
        self.state.navigation.set_content(ContentState::Loading);
        // The playlist is being reloaded (content may have changed), so invalidate the previous "全量已入队" marker.
        self.queued_playlists.remove(&id);

        let selected_api = self.state.navigation.nav.selected_api();

        let is_album = selected_api == Some("album_sublist");
        let is_radio = selected_api == Some("user_radio_sublist");

        if !is_album {
            self.playback.set_playlist_id(id);
        }

        let service = self.service.clone();
        let sender = self.state.events.sender();
        let limit = self.config.search_limit;
        tokio::spawn(async move {
            if is_album {
                let state = service.load_album(id).await;
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
                if let Some(n) = name.clone() {
                    send_event(&sender, NavigationEvent::BreadcrumbSet(n).into());
                }
                return;
            }
            let (state, detail_name, pagination) =
                service.load_playlist_detail(id, is_radio, limit).await;
            if let Some(pg) = pagination {
                send_event(
                    &sender,
                    NavigationEvent::ContentLoadedPaged {
                        content: state,
                        pagination: pg,
                        generation: 0,
                    }
                    .into(),
                );
            } else {
                send_event(&sender, NavigationEvent::ContentLoaded(state).into());
            }
            let breadcrumb = detail_name.or(name);
            if let Some(n) = breadcrumb {
                send_event(&sender, NavigationEvent::BreadcrumbSet(n).into());
            }
        });
    }

    pub(super) fn handle_song_play(&mut self, id: u64) {
        if self.playback.is_currently_playing(id) {
            self.playback.toggle_pause();
            return;
        }
        let pos = match self.state.navigation.content.as_ref() {
            ContentState::Songs(songs) => songs.iter().position(|s| s.id == id),
            _ => None,
        };
        if let Some(pos) = pos {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref() {
                if self.state.navigation.content_is_search && sonar::is_sonar_song_id(id) {
                    // Third-party search always goes into the same queue; do not reuse queues built by keyword/date
                    self.playback
                        .append_and_play_key(THIRD_PARTY_QUEUE_KEY, &songs[pos..=pos], 0);
                } else if self.state.navigation.content_is_search {
                    // NetEase Cloud search always goes into the "官方搜索" queue
                    self.playback
                        .append_and_play_key(NCM_SEARCH_QUEUE_KEY, &songs[pos..=pos], 0);
                } else {
                    let key = self.current_queue_key();
                    let lazy_id = self
                        .state
                        .navigation
                        .pagination
                        .as_ref()
                        .filter(|p| p.has_more)
                        .and_then(|p| p.api.strip_prefix("playlist:"))
                        .and_then(|s| s.parse::<u64>().ok());

                    if let Some(id) = lazy_id {
                        if self.queued_playlists.contains(&id) {
                            // The full track list was already merged into this playlist's
                            // queue (in memory or persisted), so activate the queue directly
                            // and seek to the song, avoiding rebuilding/truncating or refetching.
                            // Locate by song ID rather than content-list index: `a` inserts
                            // the next song after the current one, so the queue order no longer
                            // matches the content list, and `play_index` by content index would
                            // play the wrong song.
                            let qkey = self.playback.queue_key_for(&key);
                            self.playback.activate_queue(&qkey);
                            if let Some(qidx) =
                                self.playback.queue_songs().iter().position(|s| s.id == id)
                            {
                                self.playback.play_index(qidx);
                            } else {
                                self.playback.play_songs(&key, songs.to_vec(), pos);
                            }
                        } else {
                            // Lazily-paged playlist: play the first page immediately; the remaining tracks are merged into the same queue in the background in batches.
                            self.playback.play_songs(&key, songs.to_vec(), pos);
                            let (api, limit, total) = {
                                let p = self
                                    .state
                                    .navigation
                                    .pagination
                                    .as_ref()
                                    .expect("lazy branch implies pagination is Some");
                                (p.api.clone(), p.limit, p.total)
                            };
                            let qkey = self.playback.queue_key_for(&key);
                            let service = self.service.clone();
                            let sender = self.state.events.sender();
                            let start = songs.len() as u32;
                            tokio::spawn(async move {
                                let mut offset = start;
                                let mut completed = true;
                                loop {
                                    match service.load_more(&api, offset, limit).await {
                                        Some((ContentState::Songs(page), next_pg)) => {
                                            if page.is_empty() {
                                                break;
                                            }
                                            send_event(
                                                &sender,
                                                PlaybackEvent::QueueAppend {
                                                    key: qkey.clone(),
                                                    songs: page,
                                                }
                                                .into(),
                                            );
                                            offset = next_pg.offset + next_pg.limit;
                                            if !next_pg.has_more || u64::from(offset) >= total {
                                                break;
                                            }
                                        }
                                        _ => {
                                            completed = false;
                                            break;
                                        }
                                    }
                                }
                                if completed {
                                    send_event(
                                        &sender,
                                        PlaybackEvent::QueueLoadDone { playlist_id: id }.into(),
                                    );
                                }
                            });
                        }
                    } else {
                        self.playback.play_songs(&key, songs.to_vec(), pos);
                    }
                }
            }
            let toast_name: &str = match self.state.navigation.content.as_ref() {
                ContentState::Songs(songs) => songs.get(pos).map(|s| s.name.as_str()).unwrap_or(""),
                _ => "",
            };
            self.toast(format!("▶  {}", toast_name));
        }
    }

    pub(super) fn handle_playback_started(&mut self) {
        self.playback.on_playback_started();

        if let Some(song) = self.playback.current_song() {
            if let ContentState::Songs(songs) = self.state.navigation.content.as_ref()
                && let Some(pos) = songs.iter().position(|s| s.id == song.id)
            {
                self.state.navigation.content_selected = pos;
            }
            self.toast(format!("▶  {}", song.name));
            let song_id = song.id;

            if sonar::is_sonar_song_id(song_id) {
                let service = self.service.clone();
                let finder = self.finder.clone();
                let registry = self.sonar_songs.clone();
                let sender = self.state.events.sender();
                tokio::spawn(async move {
                    let Some((lyric_lines, tlyric_lines)) =
                        service.load_sonar_lyrics(song_id, finder, &registry).await
                    else {
                        return;
                    };
                    send_event(
                        &sender,
                        PlaybackEvent::LyricsLoaded {
                            song_id,
                            lyrics: lyric_lines,
                            translated_lyrics: tlyric_lines,
                        }
                        .into(),
                    );
                });
            } else if let Some(audio) = local_audio_path(&song) {
                // Local tracks have no lyrics on the server — their id is just a
                // path hash, so asking for it could only ever come back empty.
                let audio = audio.to_path_buf();
                let sender = self.state.events.sender();
                tokio::spawn(async move {
                    let lyrics = tokio::task::spawn_blocking(move || load_local_lyrics(&audio))
                        .await
                        .ok()
                        .flatten();
                    let Some(lyrics) = lyrics else {
                        return;
                    };
                    let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                    let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                    send_event(
                        &sender,
                        PlaybackEvent::LyricsLoaded {
                            song_id,
                            lyrics: lyric_lines,
                            translated_lyrics: tlyric_lines,
                        }
                        .into(),
                    );
                });
            } else {
                let service = self.service.clone();
                let sender = self.state.events.sender();
                tokio::spawn(async move {
                    if let Some(lyrics) = service.load_lyrics(song_id).await {
                        let lyric_lines = parse_lyric_lines(&lyrics.lyric);
                        let tlyric_lines = parse_lyric_lines(&lyrics.tlyric);
                        send_event(
                            &sender,
                            PlaybackEvent::LyricsLoaded {
                                song_id,
                                lyrics: lyric_lines,
                                translated_lyrics: tlyric_lines,
                            }
                            .into(),
                        );
                    }
                });
            }

            // Clear the cover so a new song never shows the previous one's
            // cover while its own cover is loading (or missing).
            self.playback.state.cover.clear();

            // The MV poster travels with the song the way the lyrics and the cover do: it is
            // fetched when the track starts, and every failure — no MV on the song, a detail
            // request that errors, a poster that cannot be downloaded — is silent, because the
            // panel is decoration and the page says what it has either way. `clear` bumps the
            // generation first, so a poster still on its way when the song changes lands in the
            // empty slot instead of beside the new song's lyrics.
            mv::clear();
            if song.mv != 0 {
                let client = self.service.client().clone();
                let http = self.cover_http.clone();
                let picker = self.picker.clone();
                let sender = self.state.events.sender();
                let generation = mv::generation();
                let mv_id = song.mv;
                tokio::spawn(async move {
                    if mv::fetch_and_install(&client, &http, mv_id, &picker, generation).await {
                        // The frame on screen was drawn without a poster; only a wake-up gets it
                        // drawn again.
                        send_event(&sender, AppEvent::Repaint.into());
                    }
                });
            }

            // Load cover image
            let song_id = song.id;
            let is_sonar = sonar::is_sonar_song_id(song_id);
            let own_pic = song.pic_url.clone();
            let cover = self.playback.state.cover.clone();
            let picker = self.picker.clone();
            let cache = self.service.cache().clone();
            let cover_http = self.cover_http.clone();

            if !own_pic.is_empty() || is_sonar {
                let finder = self.finder.clone();
                let registry = self.sonar_songs.clone();
                tokio::spawn(async move {
                    // Mark whose cover we are loading; a stale loader for a
                    // previously played song will be dropped below.
                    if let Ok(mut g) = cover.song_id.lock() {
                        *g = Some(song_id);
                    }

                    // Serve from cache first — never block a cached cover on
                    // re-resolving the source URL, which can fail offline or
                    // when the third-party provider is unreachable.
                    let cached = {
                        let cache = cache.clone();
                        let picker = picker.clone();
                        match cache.load_cover_async(song_id).await {
                            Some(data) => {
                                tokio::task::spawn_blocking(move || build_cover(&data, &picker))
                                    .await
                                    .ok()
                                    .flatten()
                            }
                            None => None,
                        }
                    };
                    if let Some((protocol, square)) = cached {
                        cover.install(song_id, square, protocol);
                        return;
                    }

                    // Cache miss — resolve a cover URL: own cover, else
                    // fallback search (kuwo preferred) for sonar songs without
                    // one.
                    let cover_url = if !own_pic.is_empty() {
                        Some(own_pic)
                    } else {
                        let msong = registry
                            .lock()
                            .ok()
                            .and_then(|m| m.get(&song_id).cloned())
                            .or_else(|| cache.thirdparty_song(song_id));
                        match msong {
                            Some(msong) => finder.get_cover_fallback(&msong).await,
                            None => None,
                        }
                    };
                    let Some(cover_url) = cover_url else {
                        return;
                    };

                    let small_url = if cover_url.contains('?') {
                        format!("{}&param=200y200", cover_url)
                    } else {
                        format!("{}?param=200y200", cover_url)
                    };

                    // Download the cover (async client — no blocking runtime
                    // owned by App) and process the image off the runtime. The
                    // cache was already checked above; a redundant re-check here
                    // would double the disk reads on every miss. `cover_http`
                    // carries connect/read deadlines plus a 30s total deadline,
                    // so a cover whose CDN hangs ends this task instead of
                    // leaving it parked forever.
                    let art = {
                        let Ok(resp) = cover_http.get(&small_url).send().await else {
                            return;
                        };
                        let Ok(bytes) = resp.bytes().await else {
                            return;
                        };
                        let raw = bytes.to_vec();
                        let cache = cache.clone();
                        let picker = picker.clone();
                        tokio::task::spawn_blocking(move || {
                            cache.save_cover(song_id, &raw);
                            build_cover(&raw, &picker)
                        })
                        .await
                        .ok()
                        .flatten()
                    };

                    let Some((protocol, square)) = art else {
                        return;
                    };

                    cover.install(song_id, square, protocol);
                });
            }
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                                  Helper fn                                 */
/* -------------------------------------------------------------------------- */

/// Decode cover bytes into the disc the playerbar draws, plus the square it was cut
/// from: the spin re-encodes that square at each angle it turns to, which is what
/// keeps the last angle of a turn as sharp as the first.
fn build_cover(
    data: &[u8],
    picker: &ratatui_image::picker::Picker,
) -> Option<(ratatui_image::protocol::StatefulProtocol, image::RgbaImage)> {
    let Ok(img) = image::load_from_memory(data) else {
        return None;
    };
    let (w, h) = img.dimensions();
    let size = w.min(h);
    let x = (w - size) / 2;
    let y = (h - size) / 2;
    let square = img.crop_imm(x, y, size, size).to_rgba8();
    drop(img);

    let protocol = CoverState::encode_disc(&square, 0.0, picker);
    Some((protocol, square))
}

/// The file behind a local track, if this song is one.
///
/// Follows the rule local playback resolves with: the path is `local_path`,
/// with `album` as the fallback for content cached by an older version. The
/// `is_file` check is what keeps a network song whose album merely reads like a
/// path from being treated as local.
fn local_audio_path(song: &SongInfo) -> Option<&Path> {
    if song.copyright != ncm_api::SongCopyright::Free {
        return None;
    }
    let path = Path::new(song.local_path.as_deref().unwrap_or(song.album.as_str()));
    path.is_file().then_some(path)
}

/// Lyrics for a local track, read from the `.lrc` sidecar next to the audio.
///
/// Deliberately the same shape the network path produces ([`ncm_api::Lyrics`]
/// carrying raw LRC lines), so everything downstream — `parse_lyric_lines`, the
/// `LyricsLoaded` event, the karaoke sweep — is shared rather than reimplemented.
///
/// Nothing is cached: reading a local file costs about what reading its cache
/// entry would, and an edited `.lrc` then takes effect on the next play.
fn load_local_lyrics(audio: &Path) -> Option<ncm_api::Lyrics> {
    let sidecar = sidecar_lrc_path(audio)?;
    let raw = std::fs::read_to_string(sidecar).ok()?;
    // A byte-order mark would land inside the first timestamp and cost the first
    // line of every BOM'd file.
    let lyric: Vec<String> = raw
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::to_string)
        .collect();
    // A sidecar that holds no timestamps (plain text, or only `[ti:]`-style
    // metadata) parses to nothing, and no lyrics beats wrong lyrics.
    (!parse_lyric_lines(&lyric).is_empty()).then(|| ncm_api::Lyrics {
        lyric,
        tlyric: Vec::new(),
    })
}

/// The `.lrc` sitting next to `audio`, if there is one.
///
/// Case-insensitive: on the case-sensitive filesystems this runs on, `Song.lrc`
/// and `SONG.LRC` are the same track's lyrics to whoever wrote them, and both
/// spellings are common in the wild. The exact-case sibling is tried first, so
/// only a miss pays for the directory scan.
fn sidecar_lrc_path(audio: &Path) -> Option<PathBuf> {
    let stem = audio.file_stem()?.to_str()?;
    let dir = audio.parent()?;
    let exact = dir.join(format!("{stem}.lrc"));
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case(stem))
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("lrc"))
        })
}

/// Benchmark of the cover path on the covers actually cached on this machine.
/// `cargo test --release --lib -- --ignored --nocapture cover_bench`
#[cfg(test)]
mod cover_bench {
    use std::path::PathBuf;

    use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
    use ratatui_image::{Resize, StatefulImage};

    use super::*;

    fn cached_covers() -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(cache) = dirs::cache_dir() {
            dirs.push(cache.join("boxpigma/covers"));
        }
        dirs.into_iter()
            .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten())
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg"))
            })
            .take(5)
            .collect()
    }

    #[test]
    #[ignore = "benchmark"]
    fn decoding_and_masking_a_cover_costs() {
        let covers = cached_covers();
        if covers.is_empty() {
            println!("  没有缓存封面，跳过：先播一首歌让封面落盘");
            return;
        }

        let picker = ratatui_image::picker::Picker::halfblocks();
        println!("封面（真实缓存图，{} 张）:", covers.len());
        for path in &covers {
            let Ok(data) = std::fs::read(path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            crate::bench_util::time(
                &format!("解码+裁方+圆形蒙版+协议 {name}"),
                5,
                || {
                    let art = build_cover(std::hint::black_box(&data), &picker);
                    std::hint::black_box(art);
                },
            );
        }
    }

    /// The spin's per-step cost: rotate the square, re-cut the mask, encode the
    /// protocol and render it. `[playerbar] spinning_cover` pays this once per angle
    /// step, not once per frame — the frames in between draw the protocol already in
    /// hand, which costs nothing.
    #[test]
    #[ignore = "benchmark"]
    fn spinning_a_cover_costs() {
        let covers = cached_covers();
        if covers.is_empty() {
            println!("  没有缓存封面，跳过：先播一首歌让封面落盘");
            return;
        }

        // The playerbar's cover area (the modern layout gives it 8 cells by 3 rows) and
        // a graphics terminal's protocol: on halfblocks the spin draws the turning
        // glyph instead and never reaches this path.
        let area = Rect::new(0, 0, 8, 3);
        let mut picker = ratatui_image::picker::Picker::halfblocks();
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
        // One angle step every `TURN_SECS / STEPS_PER_TURN` seconds.
        let per_second = f64::from(CoverState::STEPS_PER_TURN) / CoverState::TURN_SECS;

        println!("封面旋转（真实缓存图，{} 张，kitty 协议）:", covers.len());
        for path in &covers {
            let Ok(data) = std::fs::read(path) else {
                continue;
            };
            let Some((_, square)) = build_cover(&data, &picker) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut step = 0;
            let per_step = crate::bench_util::time(
                &format!("旋转+重新套蒙版+协议+编码 {name}"),
                40,
                || {
                    // An angle the renderer has not drawn: the cached protocol would
                    // otherwise be handed straight back.
                    step = (step + 1) % CoverState::STEPS_PER_TURN;
                    let turn = step as f32 / CoverState::STEPS_PER_TURN as f32;
                    let mut protocol =
                        CoverState::encode_disc(std::hint::black_box(&square), turn, &picker);
                    let mut buffer = Buffer::empty(area);
                    StatefulImage::new().resize(Resize::Fit(None)).render(
                        area,
                        &mut buffer,
                        &mut protocol,
                    );
                    std::hint::black_box((protocol, buffer));
                },
            );
            println!(
                "    → {per_second:.1} 个角度/秒，占单核 {:.3}%",
                crate::bench_util::core_share(per_step, per_second)
            );
        }
    }
}

/// Local lyrics: locating the `.lrc` sidecar, and feeding what it holds through
/// the same pipeline the network lyrics take.
#[cfg(test)]
mod local_lyrics_tests {
    use std::time::Duration;

    use super::*;

    /// A scratch directory this test owns; `label` keeps parallel tests apart.
    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boxpigma-local-lyrics-test-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// `parse_lyric_lines` is the shared half of the feature: the sidecar has to
    /// come out of it as timed lines, with the metadata tags and the BOM gone.
    #[test]
    fn a_differently_cased_sidecar_reaches_the_shared_lyrics_pipeline() {
        let dir = temp_dir("cased");
        let audio = dir.join("track.flac");
        std::fs::write(&audio, b"").expect("write audio placeholder");
        // BOM, CRLF and an `[ar:]` tag: the shapes real `.lrc` files come in.
        std::fs::write(
            dir.join("TRACK.LRC"),
            "\u{feff}[ar:某歌手]\r\n[00:12.50]第二行\r\n[00:01.00]第一行\r\n",
        )
        .expect("write sidecar");

        let lyrics = load_local_lyrics(&audio).expect("sidecar lyrics");
        assert!(lyrics.tlyric.is_empty());
        let lines = parse_lyric_lines(&lyrics.lyric);
        assert_eq!(lines.len(), 2, "metadata is not lyrics: {lines:?}");
        assert_eq!(lines[0].text, "第一行");
        assert_eq!(lines[0].time, Duration::from_millis(1000));
        assert_eq!(lines[1].text, "第二行");
        assert_eq!(lines[1].time, Duration::from_millis(12500));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_or_timestampless_sidecar_yields_no_lyrics() {
        let dir = temp_dir("missing");
        let audio = dir.join("track.mp3");
        std::fs::write(&audio, b"").expect("write audio placeholder");
        // Same stem, different kind of file.
        std::fs::write(dir.join("track.txt"), "[00:01.00]文本").expect("write decoy");
        // Another track's lyrics.
        std::fs::write(dir.join("other.lrc"), "[00:01.00]别人").expect("write decoy");
        assert!(load_local_lyrics(&audio).is_none());

        // A file named `.lrc` that holds no timestamps must not pass itself off
        // as lyrics.
        std::fs::write(dir.join("track.lrc"), "纯文本，没有时间戳\n").expect("write sidecar");
        assert!(load_local_lyrics(&audio).is_none());

        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod mv_panel_hook {
    use std::time::Duration;

    use super::*;
    use crate::{
        config::{Config, ProxyTarget},
        state::mv,
    };

    /// A song the hook can be started on: only its id and whether it has an MV matter here.
    fn song(id: u64, mv: u64) -> Arc<SongInfo> {
        Arc::new(SongInfo {
            id,
            name: "test".into(),
            singer: String::new(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 60_000,
            mv,
            copyright: ncm_api::SongCopyright::Free,
            local_path: None,
        })
    }

    /// The hook that starts a track owns the panel: it drops the poster the previous song left
    /// and, when the new song has an MV, asks for its own — and a request that cannot be
    /// answered leaves the slot as empty as it was. Every request here goes to the discard port,
    /// which is what a machine with no network looks like to the app.
    #[tokio::test]
    async fn a_track_change_clears_and_reloads_the_panel() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _turn = mv::fixtures::turn().await;

        let config = Config {
            proxy: "http://127.0.0.1:9".into(),
            proxy_target: ProxyTarget::Both,
            ..Config::default()
        };
        let mut app = App::new(config, false).expect("app");

        // A song with an MV, and the poster the song before it left behind.
        mv::clear();
        app.playback.state.current_song = Some(song(7, 7));
        assert!(mv::install(
            mv::generation(),
            mv::fixtures::panel(&app.picker)
        ));
        let before = mv::generation();

        app.handle_playback_started();

        assert!(mv::panel().is_none(), "the old poster must be dropped");
        let after = mv::generation();
        assert!(after > before, "and the load must belong to the new song");

        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            mv::panel().is_none(),
            "a detail request that is never answered installs nothing"
        );
        assert_eq!(
            mv::generation(),
            after,
            "and a failed load is not a track change"
        );

        // A song with no MV starts no load, and still leaves no poster behind.
        app.playback.state.current_song = Some(song(8, 0));
        assert!(mv::install(
            mv::generation(),
            mv::fixtures::panel(&app.picker)
        ));
        app.handle_playback_started();
        assert!(
            mv::panel().is_none(),
            "a song with no MV must clear the slot"
        );

        mv::clear();
    }
}
