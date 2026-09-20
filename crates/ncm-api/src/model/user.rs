use super::song::{SongCopyright, SongInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- User / cloud disk models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginInfo {
    pub code: i32,
    pub uid: u64,
    pub nickname: String,
    pub avatar_url: String,
    pub vip_type: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    pub code: i32,
    pub msg: String,
}

/// Turn the `/weapi/point/dailyTask` response into a sentence the UI can show.
///
/// A plain success carries a `point` but **no** `msg`, which used to reach the toast as an empty
/// string; `-2` is NetEase's "already signed in today", a normal state rather than a failure.
/// Anything else is a business error and keeps the raw response so `Display` can render the
/// server's own code and message.
pub(crate) fn parse_daily_task(value: &Value) -> Result<Msg, crate::error::NcmError> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    let server_msg = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let msg = match code {
        200 => match value["point"].as_i64().filter(|point| *point > 0) {
            Some(point) => format!("签到成功（云贝 {point}）"),
            None => "签到成功".to_string(),
        },
        -2 if server_msg.is_empty() => "今天已签到".to_string(),
        -2 => server_msg,
        _ => return Err(crate::error::NcmError::api(value.clone())),
    };
    Ok(Msg { code, msg })
}

#[cfg(test)]
mod daily_task_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plain_success_still_says_something() {
        // The real response for a fresh sign has `point` but no `msg`; an empty toast is the bug.
        let msg = parse_daily_task(&json!({ "code": 200 })).expect("200 应当成功");
        assert_eq!(msg.msg, "签到成功");
    }

    #[test]
    fn a_success_with_points_reports_them() {
        let msg = parse_daily_task(&json!({ "code": 200, "point": 5 })).expect("200 应当成功");
        assert_eq!(msg.msg, "签到成功（云贝 5）");
    }

    #[test]
    fn already_signed_in_is_a_normal_result() {
        let msg = parse_daily_task(&json!({ "code": -2, "msg": "今天已签到" })).expect("-2 不是错误");
        assert_eq!(msg.code, -2);
        assert_eq!(msg.msg, "今天已签到");
        let bare = parse_daily_task(&json!({ "code": -2 })).expect("-2 不是错误");
        assert_eq!(bare.msg, "今天已签到");
    }

    #[test]
    fn a_business_failure_is_an_error_that_names_the_cause() {
        let err = parse_daily_task(&json!({ "code": 301, "msg": "需要登录" })).expect_err("301 应当报错");
        let text = err.to_string();
        assert!(text.contains("301"), "错误里应带服务端 code，实得 {text:?}");
        assert!(text.contains("需要登录"), "错误里应带服务端 msg，实得 {text:?}");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudUploadResult {
    pub song_id: u64,
    pub song_name: String,
    /// The raw merged response returned by the server, for debugging / UI access to private-cloud fields
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDiskResult {
    pub songs: Vec<SongInfo>,
    pub has_more: bool,
    pub count: u64,
}

// --- User / cloud disk parsing ---

pub(crate) fn parse_login_info(value: &Value) -> Result<LoginInfo, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    if code == 200 {
        Ok(LoginInfo {
            code,
            uid: value["profile"]["userId"].as_u64().unwrap_or(0),
            nickname: value["profile"]["nickname"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            avatar_url: value["profile"]["avatarUrl"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            vip_type: value["profile"]["vipType"].as_i64().unwrap_or(0) as i32,
            msg: String::new(),
        })
    } else {
        // Risk control comes first. Its answers carry a `message`, but it only says that something
        // is wrong; a terminal cannot show the slider it demands, so the message must name the one
        // way out instead of repeating the server's wording.
        let msg = match code {
            -460 => "网易云风控：网络环境存在风险，请改用二维码登录".to_string(),
            -462 => "网易云风控：要求滑块验证，终端做不了，请改用二维码登录".to_string(),
            _ => value
                // The server uses `msg` on some endpoints and `message` on others; honour both.
                .get("msg")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| match code {
                    501 => "账号或密码错误".to_string(),
                    502 => "请切换登录方式或升级版本".to_string(),
                    10004 => "当前登录存在安全风险，请稍后再试".to_string(),
                    301 => "登录已过期".to_string(),
                    _ => format!("登录失败 (code={code})"),
                }),
        };
        Err(msg)
    }
}

pub(crate) fn parse_msg(value: &Value) -> Result<Msg, String> {
    let code = value["code"].as_i64().unwrap_or(0) as i32;
    let msg = value
        .get("msg")
        .or_else(|| value.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Msg { code, msg })
}

pub(crate) fn parse_unikey(value: &Value) -> Result<String, String> {
    value["unikey"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "unikey not found".to_string())
}

pub(crate) fn parse_cloud_upload(value: &Value) -> Result<CloudUploadResult, String> {
    let song_id = value["songId"].as_u64().or_else(|| {
        value
            .get("privateCloud")
            .and_then(|p| p.get("songId"))
            .and_then(|v| v.as_u64())
    });
    let song_name = value["songName"]
        .as_str()
        .or_else(|| {
            value
                .get("privateCloud")
                .and_then(|p| p.get("songName"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    Ok(CloudUploadResult {
        song_id: song_id.unwrap_or(0),
        song_name,
        raw: value.clone(),
    })
}

pub(crate) fn parse_cloud_disk_songs(value: &Value) -> Result<CloudDiskResult, String> {
    let array = value["data"].as_array().ok_or("data not found")?;
    let songs = array
        .iter()
        .map(|v| -> Result<SongInfo, String> {
            let simple = &v["simpleSong"];
            Ok(SongInfo {
                id: v["songId"]
                    .as_u64()
                    .or_else(|| simple["id"].as_u64())
                    .unwrap_or(0),
                name: v["songName"].as_str().unwrap_or("unknown").to_string(),
                singer: v["artist"].as_str().unwrap_or("unknown").to_string(),
                artist_id: simple
                    .get("ar")
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("id"))
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0),
                album: v["album"].as_str().unwrap_or("unknown").to_string(),
                album_id: 0,
                pic_url: simple
                    .get("al")
                    .and_then(|a| a.get("picUrl"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string(),
                duration: simple["dt"].as_u64().unwrap_or(0),
                copyright: SongCopyright::Unknown,
                local_path: None,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = value
        .get("hasMore")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let count = value.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok(CloudDiskResult {
        songs,
        has_more,
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_msg() {
        let v = json!({"code": 200, "msg": "success"});
        let msg = parse_msg(&v).unwrap();
        assert_eq!(msg.code, 200);
        assert_eq!(msg.msg, "success");

        let v2 = json!({"code": 500, "message": "error occurred"});
        let msg2 = parse_msg(&v2).unwrap();
        assert_eq!(msg2.code, 500);
        assert_eq!(msg2.msg, "error occurred");
    }

    #[test]
    fn test_parse_login_info_success() {
        let v = json!({
            "code": 200,
            "profile": {
                "userId": 12345,
                "nickname": "test_user",
                "avatarUrl": "http://avatar.png",
                "vipType": 1
            }
        });
        let info = parse_login_info(&v).unwrap();
        assert_eq!(info.code, 200);
        assert_eq!(info.uid, 12345);
        assert_eq!(info.nickname, "test_user");
        assert_eq!(info.avatar_url, "http://avatar.png");
        assert_eq!(info.vip_type, 1);
    }

    #[test]
    fn test_parse_login_info_failure() {
        let v = json!({
            "code": 400,
            "msg": "login failed"
        });
        let err = parse_login_info(&v).unwrap_err();
        assert_eq!(err, "login failed");
    }

    #[test]
    fn test_parse_unikey() {
        let v = json!({"unikey": "abc123"});
        assert_eq!(parse_unikey(&v).unwrap(), "abc123");

        let v2 = json!({});
        assert!(parse_unikey(&v2).is_err());
    }
}

#[cfg(test)]
mod login_error_tests {
    use super::*;
    use serde_json::json;

    fn error_text(value: serde_json::Value) -> String {
        parse_login_info(&value).expect_err("应当报错")
    }

    #[test]
    fn risk_control_tells_the_user_the_way_out() {
        // Verified against the live API: -460 carried only `message`, and used to lose it entirely.
        let text = error_text(json!({ "code": -460, "message": "检测到您的网络环境存在风险，请稍后再试" }));
        assert!(text.contains("二维码"), "风控错误必须给出可行的出路，实得 {text:?}");
        let text = error_text(json!({ "code": -462 }));
        assert!(text.contains("二维码"), "滑块验证在终端里做不了，必须指向二维码，实得 {text:?}");
    }

    #[test]
    fn the_server_message_is_used_when_it_has_one() {
        let text = error_text(json!({ "code": 999, "message": "服务端原话" }));
        assert!(text.contains("服务端原话"), "服务端的 message 字段不能被丢弃，实得 {text:?}");
    }

    #[test]
    fn a_plain_wrong_password_still_reads_normally() {
        let text = error_text(json!({ "code": 501 }));
        assert_eq!(text, "账号或密码错误");
    }
}
