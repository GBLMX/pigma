use super::{str_val, u64_val};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- MV (music video) models ---

/// A NetEase MV — the video belonging to a song, reachable from `SongInfo::mv`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvInfo {
    pub id: u64,
    pub name: String,
    pub artist_id: u64,
    pub artist_name: String,
    /// Poster image, shown next to the MV info.
    pub cover: String,
    /// Duration in milliseconds.
    pub duration: u64,
    /// Release date, as the server writes it: `YYYY-MM-DD`.
    pub publish_time: String,
    /// Intro text: `desc` when the server fills it in, otherwise `briefDesc` (many MVs answer
    /// `desc: null`, and the short blurb is all there is).
    pub desc: String,
    pub play_count: u64,
    pub sub_count: u64,
    pub share_count: u64,
    /// Sent by `/api/mv/detail`; `/api/v1/mv/detail` omits it.
    pub like_count: u64,
    pub comment_count: u64,
    /// Renditions the MV has, ascending by resolution.
    pub resolutions: Vec<MvResolution>,
}

/// One rendition of an MV, taken from a detail response's `brs`.
///
/// The two live detail endpoints disagree about that field's shape. `/api/mv/detail` sends an
/// object keyed by resolution whose values are signed, expiring direct URLs —
/// `{"240": "http://…/x.mp4?wsSecret=…&wsTime=…", "480": "http://…/y.mp4?…"}` — while
/// `/api/v1/mv/detail` sends a list without URLs —
/// `[{"size": 20976896.0, "br": 240, "point": 0}, …]`. Both land here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvResolution {
    /// Vertical resolution in pixels (`br` / the object key): `240` / `480` / `720` / `1080`.
    pub resolution: u32,
    /// Size in bytes; `0` when the response carried only URLs.
    pub size: u64,
    /// Signed direct URL; empty when the response carried only sizes.
    pub url: String,
}

/// A playable MV source, as answered by `/api/song/enhance/play/mv/url`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvUrl {
    pub id: u64,
    pub url: String,
    /// Resolution of `url` (`r`). The server clamps the request down to the best rendition the
    /// MV has, so asking for `1080` can answer `480`.
    pub resolution: u32,
    /// Size in bytes.
    pub size: u64,
    /// Lifetime of the signed URL, in seconds (`expi`).
    pub expire_secs: u64,
}

// --- MV parsing ---

pub(crate) fn parse_mv_detail(value: &Value) -> Result<MvInfo, String> {
    let data = value
        .get("data")
        .filter(|d| d.is_object())
        .ok_or("data not found")?;

    let desc = ["desc", "briefDesc"]
        .iter()
        .find_map(|key| data[*key].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string();

    Ok(MvInfo {
        id: u64_val(data, "id"),
        name: str_val(data, "name"),
        artist_id: u64_val(data, "artistId"),
        artist_name: str_val(data, "artistName"),
        cover: data["cover"].as_str().unwrap_or("").to_string(),
        duration: u64_val(data, "duration"),
        publish_time: data["publishTime"].as_str().unwrap_or("").to_string(),
        desc,
        play_count: u64_val(data, "playCount"),
        sub_count: u64_val(data, "subCount"),
        share_count: u64_val(data, "shareCount"),
        like_count: u64_val(data, "likeCount"),
        comment_count: u64_val(data, "commentCount"),
        resolutions: parse_mv_resolutions(data.get("brs")),
    })
}

/// Read `brs`, which is a resolution → URL map on `/api/mv/detail` and a `{br, size}` list on
/// `/api/v1/mv/detail`. Unknown shapes simply yield nothing.
fn parse_mv_resolutions(brs: Option<&Value>) -> Vec<MvResolution> {
    let mut resolutions: Vec<MvResolution> = match brs {
        Some(Value::Object(map)) => map
            .iter()
            .filter_map(|(resolution, url)| {
                let url = url.as_str().filter(|u| !u.is_empty())?;
                Some(MvResolution {
                    resolution: resolution.parse().unwrap_or(0),
                    size: 0,
                    url: url.to_string(),
                })
            })
            .collect(),
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|entry| {
                Some(MvResolution {
                    resolution: entry["br"].as_u64()? as u32,
                    // Sizes arrive as JSON floats (`20976896.0`), which `as_u64` rejects.
                    size: entry["size"]
                        .as_u64()
                        .or_else(|| entry["size"].as_f64().map(|size| size as u64))
                        .unwrap_or(0),
                    url: String::new(),
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    resolutions.sort_by_key(|rendition| rendition.resolution);
    resolutions
}

pub(crate) fn parse_mv_url(value: &Value) -> Result<Vec<MvUrl>, String> {
    let data = value
        .get("data")
        .filter(|d| d.is_object())
        .ok_or("data not found")?;

    // One call asks for one resolution (`r`), so this yields at most one entry: a resolution the
    // MV does not have comes back as `{"url": null, "r": 0, "size": 0, "code": 404}`.
    let url = match data["url"].as_str().filter(|url| !url.is_empty()) {
        Some(url) => url.to_string(),
        None => return Ok(Vec::new()),
    };

    Ok(vec![MvUrl {
        id: u64_val(data, "id"),
        url,
        resolution: u64_val(data, "r") as u32,
        size: u64_val(data, "size"),
        expire_secs: u64_val(data, "expi"),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Trimmed from the live `/api/mv/detail` answer for MV 376199 (the MV of song 347230).
    #[test]
    fn test_parse_mv_detail() {
        let v = json!({
            "subed": false,
            "code": 200,
            "data": {
                "id": 376199,
                "name": "海阔天空",
                "artistId": 11127,
                "artistName": "Beyond",
                "briefDesc": "",
                "desc": "《海阔天空》是黄家驹为Beyond成立十周年而作的",
                "cover": "http://p3.music.126.net/S8InCa4o-pFJszhUvI-NPQ==/3247957351196805.jpg",
                "coverId": 3247957351196805u64,
                "playCount": 13708080,
                "subCount": 140970,
                "shareCount": 15511,
                "likeCount": 198462,
                "commentCount": 10385,
                "duration": 317490,
                "nType": 0,
                "publishTime": "1993-09-09",
                "brs": {
                    "240": "http://vodkgeyttp8.vod.126.net/cloudmusic/x/eb672f24c10e180bbb62058cd0c50060.mp4?wsSecret=1a&wsTime=1789937563",
                    "480": "http://vodkgeyttp8.vod.126.net/cloudmusic/x/5508b93dd0abdefe41ce48d54540aca6.mp4?wsSecret=2b&wsTime=1789937563",
                },
                "artists": [{"id": 11127, "name": "Beyond"}],
                "commentThreadId": "R_MV_5_376199",
            },
        });

        let mv = parse_mv_detail(&v).unwrap();
        assert_eq!(mv.id, 376199);
        assert_eq!(mv.name, "海阔天空");
        assert_eq!(mv.artist_id, 11127);
        assert_eq!(mv.artist_name, "Beyond");
        assert_eq!(mv.duration, 317_490);
        assert_eq!(mv.publish_time, "1993-09-09");
        assert_eq!(mv.desc, "《海阔天空》是黄家驹为Beyond成立十周年而作的");
        assert_eq!(mv.play_count, 13_708_080);
        assert_eq!(mv.like_count, 198_462);
        assert!(mv.cover.starts_with("http://p3.music.126.net/"));
        assert_eq!(mv.resolutions.len(), 2);
        assert_eq!(mv.resolutions[0].resolution, 240);
        assert_eq!(mv.resolutions[1].resolution, 480);
        assert!(mv.resolutions[0].url.contains("wsSecret="));
    }

    /// `/api/v1/mv/detail` answers with the size ladder and `desc: null`; the short blurb is
    /// then the only intro there is.
    #[test]
    fn test_parse_mv_detail_v1_ladder_and_short_desc() {
        let v = json!({
            "code": 200,
            "data": {
                "id": 14206146,
                "name": "Por Ti",
                "artistId": 30184674,
                "artistName": "Marta Sango",
                "briefDesc": "现场版",
                "desc": null,
                "cover": "http://p4.music.126.net/71kQLA8_PxFyneUiZYiuvQ==/109951165540928902.jpg",
                "coverId_str": "109951165540928902",
                "duration": 272000,
                "publishTime": "2019-12-26",
                "price": null,
                "brs": [
                    {"size": 17989710.0, "br": 240, "point": 0},
                    {"size": 28799486.0, "br": 480, "point": 0},
                    {"size": 45061240.0, "br": 720, "point": 0},
                    {"size": 75180656.0, "br": 1080, "point": 0},
                ],
                "artists": [{"id": 30184674, "name": "Marta Sango", "img1v1Url": null}],
                "videoGroup": [{"id": 12100, "name": "流行", "type": 0}],
            },
        });

        let mv = parse_mv_detail(&v).unwrap();
        assert_eq!(mv.desc, "现场版");
        assert_eq!(mv.like_count, 0);
        assert_eq!(
            mv.resolutions
                .iter()
                .map(|r| (r.resolution, r.size))
                .collect::<Vec<_>>(),
            [
                (240, 17_989_710),
                (480, 28_799_486),
                (720, 45_061_240),
                (1080, 75_180_656)
            ]
        );
        assert!(mv.resolutions.iter().all(|r| r.url.is_empty()));
    }

    #[test]
    fn test_parse_mv_detail_without_data() {
        // An MV id the server does not know answers `{"code":404}` with no `data`.
        assert!(parse_mv_detail(&json!({"code": 404})).is_err());
    }

    /// Trimmed from the live `/api/song/enhance/play/mv/url` answer for MV 376199.
    #[test]
    fn test_parse_mv_url() {
        let v = json!({
            "code": 200,
            "data": {
                "id": 376199,
                "url": "http://vodkgeyttp8.vod.126.net/cloudmusic/x/mv/376199/5508b93dd0abdefe41ce48d54540aca6.mp4?wsSecret=fe412fa06dc074e40269d6786ddda9c4&wsTime=1789937563",
                "r": 480,
                "size": 33536696,
                "md5": "",
                "code": 200,
                "expi": 3600,
                "fee": 0,
                "mvFee": 0,
                "st": 0,
                "promotionVo": null,
                "msg": "",
            },
        });

        let urls = parse_mv_url(&v).unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].id, 376199);
        assert_eq!(urls[0].resolution, 480);
        assert_eq!(urls[0].size, 33_536_696);
        assert_eq!(urls[0].expire_secs, 3600);
        assert!(urls[0].url.contains("wsSecret="));
    }

    /// `r=0` answers `{"url": null, "r": 0, "size": 0, "code": 404}` — the MV has no such
    /// rendition, which is not a parse failure.
    #[test]
    fn test_parse_mv_url_without_a_rendition() {
        let v = json!({
            "code": 200,
            "data": {
                "id": 5436672,
                "url": null,
                "r": 0,
                "size": 0,
                "md5": null,
                "code": 404,
                "expi": 0,
                "fee": 0,
                "mvFee": 0,
                "st": 0,
                "promotionVo": null,
                "msg": null,
            },
        });

        assert!(parse_mv_url(&v).unwrap().is_empty());
        assert!(parse_mv_url(&json!({"code": 404})).is_err());
    }
}
