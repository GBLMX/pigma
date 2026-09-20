use super::{
    SongContext,
    song::{SongInfo, parse_song_info_array},
    str_val, u64_val, value_get,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- Singer models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingerInfo {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
}

/// An artist's profile: what the artist page draws around the lists.
///
/// `/weapi/v1/artist/{id}` returns this object and the artist's hot songs in one
/// response, so both are parsed from the same payload instead of asking twice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistDetail {
    pub id: u64,
    pub name: String,
    /// Other names the artist goes by, e.g. `["Jay Chou", "周董"]`. Empty for most artists.
    pub alias: Vec<String>,
    /// Biography. The API sends an empty string for artists without one.
    pub brief_desc: String,
    /// Artist portrait (`picUrl`); `img1v1Url` is the list avatar and only a fallback.
    pub pic_url: String,
    /// Album count as the API reports it — the album list itself is paged, so this can
    /// exceed what one page returns.
    pub album_size: u64,
    /// Song count as the API reports it, for the same reason.
    pub music_size: u64,
    pub hot_songs: Vec<SongInfo>,
}

/// One row of an artist's album list: the `hotAlbums` of `/weapi/artist/albums/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistAlbum {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
    /// Number of tracks on the album.
    pub size: u64,
    /// Release date, in milliseconds.
    pub publish_time: u64,
}

// --- Singer parsing ---

pub(crate) fn parse_singer_info(value: &Value, path: &[&str]) -> Result<Vec<SingerInfo>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found", path))?;

    Ok(array
        .iter()
        .map(|v| SingerInfo {
            id: v["id"].as_u64().unwrap_or(0),
            name: v["name"].as_str().unwrap_or("unknown").to_string(),
            pic_url: {
                let url = v["img1v1Url"].as_str().unwrap_or("").to_string();
                if url.ends_with("5639395138885805.jpg") {
                    String::new()
                } else {
                    url
                }
            },
        })
        .collect())
}

/// Parse the `artist` object plus `hotSongs` of `/weapi/v1/artist/{id}`.
pub(crate) fn parse_artist_detail(value: &Value) -> Result<ArtistDetail, String> {
    let artist = value
        .get("artist")
        .ok_or_else(|| String::from("artist not found"))?;

    let alias = artist["alias"]
        .as_array()
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let pic_url = match str_val(artist, "picUrl") {
        url if url.is_empty() => str_val(artist, "img1v1Url"),
        url => url,
    };

    Ok(ArtistDetail {
        id: u64_val(artist, "id"),
        name: str_val(artist, "name"),
        alias,
        brief_desc: artist["briefDesc"].as_str().unwrap_or("").to_string(),
        pic_url,
        album_size: u64_val(artist, "albumSize"),
        music_size: u64_val(artist, "musicSize"),
        hot_songs: parse_song_info_array(value, &["hotSongs"], SongContext::Singer)?,
    })
}

/// Parse a paged album list, e.g. the `hotAlbums` of `/weapi/artist/albums/{id}`.
pub(crate) fn parse_artist_albums(value: &Value, path: &[&str]) -> Result<Vec<ArtistAlbum>, String> {
    let array = value_get(value, path)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("path {:?} not found", path))?;

    Ok(array
        .iter()
        .map(|v| ArtistAlbum {
            id: u64_val(v, "id"),
            name: str_val(v, "name"),
            pic_url: str_val(v, "picUrl"),
            size: u64_val(v, "size"),
            publish_time: u64_val(v, "publishTime"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Field shapes taken from a live `/weapi/v1/artist/6452` response, trimmed to the
    /// keys the parser reads: the profile fields sit under `artist`, the songs beside it.
    #[test]
    fn parses_artist_detail() {
        let value = json!({
            "artist": {
                "id": 6452,
                "name": "周杰伦",
                "alias": ["Jay Chou", "周董"],
                "briefDesc": "华语流行乐男歌手。",
                "picUrl": "https://p3.music.126.net/a.jpg",
                "img1v1Url": "https://p3.music.126.net/b.jpg",
                "albumSize": 44,
                "musicSize": 568
            },
            "hotSongs": [
                {
                    "id": 210049,
                    "name": "布拉格广场",
                    "dt": 294600,
                    "ar": [{"id": 7219, "name": "蔡依林"}],
                    "al": {"id": 21349, "name": "看我72变", "picUrl": "https://p3.music.126.net/c.jpg"}
                }
            ]
        });

        let detail = parse_artist_detail(&value).unwrap();
        assert_eq!(detail.id, 6452);
        assert_eq!(detail.name, "周杰伦");
        assert_eq!(detail.alias, vec!["Jay Chou", "周董"]);
        assert_eq!(detail.brief_desc, "华语流行乐男歌手。");
        assert_eq!(detail.pic_url, "https://p3.music.126.net/a.jpg");
        assert_eq!((detail.album_size, detail.music_size), (44, 568));
        assert_eq!(detail.hot_songs.len(), 1);
        assert_eq!(detail.hot_songs[0].name, "布拉格广场");
    }

    /// An artist without a portrait keeps the one the lists use, and an empty alias list
    /// parses as empty rather than as `[""]`.
    #[test]
    fn artist_detail_falls_back_to_the_list_avatar() {
        let value = json!({
            "artist": {
                "id": 1,
                "name": "无名",
                "alias": [],
                "briefDesc": "",
                "picUrl": "",
                "img1v1Url": "https://p4.music.126.net/list.jpg"
            },
            "hotSongs": []
        });

        let detail = parse_artist_detail(&value).unwrap();
        assert_eq!(detail.pic_url, "https://p4.music.126.net/list.jpg");
        assert!(detail.alias.is_empty());
        assert!(detail.brief_desc.is_empty());
        assert!(detail.hot_songs.is_empty());
    }

    #[test]
    fn parses_artist_albums() {
        let value = json!({
            "hotAlbums": [
                {
                    "id": 274336916,
                    "name": "即兴曲",
                    "picUrl": "https://p4.music.126.net/d.jpg",
                    "size": 1,
                    "publishTime": 1749139200000u64
                },
                {
                    "id": 147779282,
                    "name": "最伟大的作品",
                    "picUrl": "https://p3.music.126.net/e.jpg",
                    "size": 12,
                    "publishTime": 1657814400000u64
                }
            ]
        });

        let albums = parse_artist_albums(&value, &["hotAlbums"]).unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[1].name, "最伟大的作品");
        assert_eq!(albums[1].size, 12);
        assert_eq!(albums[1].publish_time, 1657814400000u64);
    }

    #[test]
    fn missing_album_list_is_an_error() {
        assert!(parse_artist_albums(&json!({"code": 200}), &["hotAlbums"]).is_err());
    }
}
