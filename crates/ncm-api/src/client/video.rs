use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

/// Resolution asked for when the caller has none in mind. The server clamps it down to the best
/// rendition the MV actually has, so this is a wish rather than a requirement.
const DEFAULT_MV_RESOLUTION: u32 = 1080;

impl NcmClient {
    // ===== MV (music video) =====

    /// Get MV details — title, poster, duration, artist, publish date, intro and renditions
    ///
    /// * `mv_id` — MV ID, i.e. [`SongInfo::mv`] (`0` means the song has no MV; the legacy
    ///   `/weapi/search/get` shape calls the same field `mvid`)
    pub async fn mv_detail(&self, mv_id: u64) -> Result<MvInfo, NcmError> {
        let id = mv_id.to_string();
        let params = vec![("id", id.as_str())];
        let result = self.request_weapi("/api/mv/detail", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_mv_detail(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get MV playback URLs
    ///
    /// One call answers with one resolution, so the list holds a single entry. `/api/mv/url`
    /// (what the older clients used) is gone — the live web player posts to
    /// `/api/song/enhance/play/mv/url` with these same `id`/`r` parameters.
    ///
    /// * `mv_id` — MV ID
    /// * `resolution` — requested vertical resolution (`240` / `480` / `720` / `1080`);
    ///   defaults to [`DEFAULT_MV_RESOLUTION`]
    pub async fn mv_url(
        &self,
        mv_id: u64,
        resolution: Option<u32>,
    ) -> Result<Vec<MvUrl>, NcmError> {
        let id = mv_id.to_string();
        let r = resolution.unwrap_or(DEFAULT_MV_RESOLUTION).to_string();
        let params = vec![("id", id.as_str()), ("r", r.as_str())];
        let result = self
            .request_weapi("/api/song/enhance/play/mv/url", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        let urls = parse_mv_url(&value).map_err(|e| NcmError::parse(e, &value))?;
        if urls.is_empty() {
            return Err(NcmError::message("这个 MV 没有可播放的清晰度"));
        }
        Ok(urls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole chain against the live service: song → its MV id → MV details → playback URL.
    /// Ignored by default because it needs the network; run it with
    /// `cargo test -p ncm-api --lib mv_chain -- --ignored --nocapture`.
    ///
    /// Song 347230 (Beyond — 海阔天空) carries MV 376199; both ids come from live responses.
    #[test]
    #[ignore]
    fn mv_chain_from_a_song_to_a_playback_url() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = NcmClient::new().unwrap();

        rt.block_on(async {
            let songs = client.songs_detail(&[347_230]).await.unwrap();
            assert_eq!(songs[0].mv, 376_199, "歌曲详情里应带 MV id");

            let mv = client.mv_detail(songs[0].mv).await.unwrap();
            assert_eq!(mv.name, "海阔天空");
            assert_eq!(mv.artist_name, "Beyond");
            assert!(mv.duration > 0 && !mv.cover.is_empty());
            assert!(!mv.resolutions.is_empty());

            // 1080 is a wish: this MV only goes up to 480.
            let urls = client.mv_url(songs[0].mv, None).await.unwrap();
            assert_eq!(urls.len(), 1);
            assert_eq!(urls[0].resolution, 480);
            assert!(urls[0].url.starts_with("http"));
            assert!(urls[0].size > 0 && urls[0].expire_secs > 0);
            println!("{mv:?}\n{:#?}", urls[0]);

            // `r=0` matches no rendition: the server answers `url: null`, which has to reach the
            // caller as a finished sentence rather than an empty list.
            let err = client
                .mv_url(songs[0].mv, Some(0))
                .await
                .expect_err("r=0 没有可播画质")
                .to_string();
            assert_eq!(err, "这个 MV 没有可播放的清晰度");

            // An MV id the server does not know is a business error, not a parse failure.
            assert!(client.mv_detail(1).await.is_err());
        });
    }
}
