use super::NcmClient;
use crate::{error::NcmError, model::*};
use serde_json::Value;

impl NcmClient {
    // ===== Artist =====

    /// Get an artist's hot songs
    ///
    /// * `id` — artist ID
    pub async fn singer_songs(&self, id: u64) -> Result<Vec<SongInfo>, NcmError> {
        // `/weapi/v1/artist/{id}` hands back the profile and the hot songs together;
        // `artist_detail` owns that payload, so this is a view onto it.
        Ok(self.artist_detail(id).await?.hot_songs)
    }

    /// Get an artist's profile (biography, portrait, sizes) and hot songs
    ///
    /// * `id` — artist ID
    pub async fn artist_detail(&self, id: u64) -> Result<ArtistDetail, NcmError> {
        let path = format!("/weapi/v1/artist/{}", id);
        let result = self.request_weapi(&path, &[]).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_artist_detail(&value).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get an artist's albums, newest first. `more` in the response says whether an
    /// offset past this page has anything in it.
    ///
    /// * `id` — artist ID
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn artist_albums(
        &self,
        id: u64,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<ArtistAlbum>, NcmError> {
        let path = format!("/weapi/artist/albums/{}", id);
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self.request_weapi(&path, &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_artist_albums(&value, &["hotAlbums"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get all songs of an artist
    ///
    /// * `id` — artist ID
    /// * `order` — `"hot"` (trending) or `"time"` (chronological)
    /// * `offset` — offset
    /// * `limit` — count
    pub async fn singer_all_songs(
        &self,
        id: u64,
        order: &str,
        offset: u16,
        limit: u16,
    ) -> Result<Vec<SongInfo>, NcmError> {
        let id_str = id.to_string();
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("id", id_str.as_str()),
            ("private_cloud", "true"),
            ("work_type", "1"),
            ("order", order),
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
        ];
        let result = self
            .request_weapi("/weapi/v1/artist/songs", &params)
            .await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_song_info_array(&value, &["songs"], SongContext::SingerSongs)
            .map_err(|e| NcmError::parse(e, &value))
    }

    /// Get hot/trending artists
    pub async fn top_artists(&self, offset: u16, limit: u16) -> Result<Vec<SingerInfo>, NcmError> {
        let offset_str = offset.to_string();
        let limit_str = limit.to_string();
        let params = vec![
            ("offset", offset_str.as_str()),
            ("limit", limit_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/artist/top", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        parse_singer_info(&value, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }

    /// Get the artist chart (leaderboard)
    ///
    /// * `r#type` — chart type (1-Chinese, 2-Western, 3-Korean, 4-Japanese)
    pub async fn toplist_artist(&self, r#type: u8) -> Result<Vec<SingerInfo>, NcmError> {
        let limit_str = 100u16.to_string();
        let offset_str = 0u16.to_string();
        let type_str = r#type.to_string();
        let params = vec![
            ("type", type_str.as_str()),
            ("limit", limit_str.as_str()),
            ("offset", offset_str.as_str()),
            ("total", "true"),
        ];
        let result = self.request_weapi("/api/toplist/artist", &params).await?;
        let value: Value = serde_json::from_str(&result)?;
        Self::check_api_code(&value)?;
        // Response: { code: 200, list: { artists: [...] } }
        let list = value
            .get("list")
            .ok_or_else(|| NcmError::parse(String::from("list not found"), &value))?;
        parse_singer_info(list, &["artists"]).map_err(|e| NcmError::parse(e, &value))
    }
}
