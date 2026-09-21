# ncm-api

网易云音乐 API 的 Rust 封装，提供 weapi / eapi 双协议加密、Cookie 会话管理、歌词解析等功能。

## 快速开始

```rust
use ncm_api::NcmClient;

#[tokio::main]
async fn main() -> Result<(), ncm_api::NcmError> {
    let client = NcmClient::new()?;

    // 登录
    let info = client.login("your_phone", "your_password").await?;
    println!("欢迎, {}", info.nickname);

    // 搜索
    let result = client.search_song("周杰伦", 0, 20).await?;
    println!("找到 {} 首歌曲", result.total);

    Ok(())
}
```

## 构建器

```rust
use ncm_api::{NcmClient, NcmClientBuilder};
use std::path::PathBuf;
use std::time::Duration;

let client = NcmClient::builder()
    .cookie_path(PathBuf::from("/tmp/cookies.json"))  // Cookie 持久化路径
    .timeout(Duration::from_secs(60))                 // 请求超时
    .proxy("socks5://127.0.0.1:1080")                // 代理 (http/https/socks5)
    .user_agent("Mozilla/5.0 ...")                    // 自定义 UA
    .build()?;
```

## 错误类型

```rust
pub enum NcmError {
    Http(reqwest::Error),                    // 网络请求失败
    Api { code: i32, message: String },      // API 业务错误
    Json(serde_json::Error),                 // JSON 解析失败
    Crypto(String),                          // 加密/解密错误
    Session(String),                         // 会话/Cookie 异常
    Io(std::io::Error),                      // 文件 I/O 错误
}
```

---

## 数据模型

<details>
<summary><b>SongInfo</b> — 歌曲信息</summary>

```rust
pub struct SongInfo {
    pub id: u64,             // 歌曲 ID
    pub name: String,        // 歌曲名称
    pub singer: String,      // 歌手名称
    pub artist_id: u64,      // 歌手 ID
    pub album: String,       // 专辑名称
    pub album_id: u64,       // 专辑 ID
    pub pic_url: String,     // 封面图 URL
    pub duration: u64,       // 时长（毫秒）
    pub mv: u64,             // MV ID（0 = 没有 MV；旧版搜索接口叫 mvid）
    pub copyright: SongCopyright,  // 版权状态
}
```

</details>

<details>
<summary><b>MvInfo</b> — MV（音乐视频）信息</summary>

```rust
pub struct MvInfo {
    pub id: u64,
    pub name: String,           // 标题
    pub artist_id: u64,
    pub artist_name: String,
    pub cover: String,          // 海报 URL
    pub duration: u64,          // 时长（毫秒）
    pub publish_time: String,   // 发布时间
    pub desc: String,           // 简介（desc，缺失时用 briefDesc）
    pub play_count: u64,
    pub sub_count: u64,
    pub share_count: u64,
    pub like_count: u64,
    pub comment_count: u64,
    pub resolutions: Vec<MvResolution>,  // 该 MV 有的清晰度
}
```

</details>

<details>
<summary><b>MvResolution</b> — MV 的一个清晰度</summary>

```rust
pub struct MvResolution {
    pub resolution: u32,  // 竖向分辨率：240 / 480 / 720 / 1080
    pub size: u64,        // 字节数（`/api/v1/mv/detail` 才会给，否则 0）
    pub url: String,      // 签名直链（`/api/mv/detail` 才会给，否则为空）
}
```

</details>

<details>
<summary><b>MvUrl</b> — MV 播放地址</summary>

```rust
pub struct MvUrl {
    pub id: u64,
    pub url: String,          // 直链（带 wsSecret / wsTime，有时效）
    pub resolution: u32,      // 实际画质
    pub size: u64,            // 字节数
    pub expire_secs: u64,     // 直链有效期（秒）
}
```

</details>

<details>
<summary><b>SongCopyright</b> — 版权状态枚举</summary>

```rust
pub enum SongCopyright {
    Free,              // 免费
    VipOnly,           // VIP 专享
    Payment,           // 付费
    VipOnlyHighRate,   // VIP 专享（高品质）
    Unavailable,       // 不可用
    Unknown,           // 未知
}
```

</details>

<details>
<summary><b>SongQuality</b> — 音质等级枚举</summary>

```rust
pub enum SongQuality {
    Standard,      // 标准 (≤128kbps, aac)
    Higher,        // 较高 (≤192kbps, aac)
    Extreme,       // 极高 (≤320kbps, aac)
    Lossless,      // 无损 (≤999kbps, flac)
    HiRes,         // Hi-Res (≤1.9Mbps, flac)
    Surround,      // 环绕声
    AudioVivid,    // 至臻享声
    Master,        // 臻品母带
}
```

</details>

<details>
<summary><b>SongUrl</b> — 播放地址</summary>

```rust
pub struct SongUrl {
    pub id: u64,              // 歌曲 ID
    pub url: String,          // 播放地址（可能为空字符串）
    pub rate: u32,            // 实际码率 (bps)
    pub quality: SongQuality, // 实际音质
}
```

</details>

<details>
<summary><b>SongList</b> — 歌单/专辑摘要</summary>

```rust
pub struct SongList {
    pub id: u64,               // 歌单 ID
    pub name: String,          // 歌单名称
    pub cover_img_url: String, // 封面图 URL
    pub author: String,        // 创建者名称
}
```

</details>

<details>
<summary><b>PlayListDetail</b> — 歌单详情</summary>

```rust
pub struct PlayListDetail {
    pub id: u64,                  // 歌单 ID
    pub name: String,             // 歌单名称
    pub cover_img_url: String,    // 封面图 URL
    pub description: String,      // 描述
    pub create_time: u64,         // 创建时间戳（毫秒）
    pub track_update_time: u64,   // 最近更新时间戳（毫秒）
    pub songs: Vec<SongInfo>,     // 包含的歌曲（最多 1000 首）
}
```

</details>

<details>
<summary><b>AlbumDetail</b> — 专辑详情</summary>

```rust
pub struct AlbumDetail {
    pub id: u64,                  // 专辑 ID
    pub name: String,             // 专辑名称
    pub pic_url: String,          // 封面图 URL
    pub description: String,      // 简介
    pub publish_time: u64,        // 发行时间戳（毫秒）
    pub artist_id: u64,           // 歌手 ID
    pub artist_name: String,      // 歌手名称
    pub artist_pic_url: String,   // 歌手头像 URL
    pub songs: Vec<SongInfo>,     // 包含的歌曲
}
```

</details>

<details>
<summary><b>SingerInfo</b> — 歌手信息</summary>

```rust
pub struct SingerInfo {
    pub id: u64,        // 歌手 ID
    pub name: String,   // 歌手名称
    pub pic_url: String, // 头像 URL
}
```

</details>

<details>
<summary><b>Lyrics</b> — 歌词</summary>

```rust
pub struct Lyrics {
    pub lyric: Vec<String>,    // 原文歌词行
    pub tlyric: Vec<String>,   // 翻译歌词行（可能为空）
}
```

</details>

<details>
<summary><b>LoginInfo</b> — 登录信息</summary>

```rust
pub struct LoginInfo {
    pub code: i32,           // 状态码 (200=成功)
    pub uid: u64,            // 用户 ID
    pub nickname: String,    // 昵称
    pub avatar_url: String,  // 头像 URL
    pub vip_type: i32,       // VIP 类型 (0=无, 1=黑胶, 2=黑胶SVIP)
    pub msg: String,         // 提示消息
}
```

</details>

<details>
<summary><b>Msg</b> — 通用响应</summary>

```rust
pub struct Msg {
    pub code: i32,    // 状态码 (200=成功)
    pub msg: String,  // 消息内容
}
```

</details>

<details>
<summary><b>TopList</b> — 排行榜</summary>

```rust
pub struct TopList {
    pub id: u64,             // 排行榜 ID
    pub name: String,        // 名称
    pub update: String,      // 更新频率描述
    pub description: String, // 描述
    pub cover: String,       // 封面图 URL
}
```

</details>

<details>
<summary><b>BannersInfo</b> — 首页 Banner</summary>

```rust
pub struct BannersInfo {
    pub pic: String,             // 图片 URL
    pub target_id: u64,          // 关联目标 ID
    pub target_type: TargetType, // 目标类型
}

pub enum TargetType {
    Song,     // 1 — 歌曲
    Album,    // 10 — 专辑
    Unknown,  // 其他
}
```

</details>

<details>
<summary><b>SearchResult</b> — 搜索结果</summary>

```rust
pub struct SearchResult {
    pub songs: Vec<SongInfo>,  // 歌曲列表
    pub total: u32,            // 总结果数
}
```

</details>

<details>
<summary><b>HotSearchItem</b> — 热搜词条目</summary>

```rust
pub struct HotSearchItem {
    pub keyword: String,  // 搜索关键词
    pub icon_type: i32,   // 图标类型 (0=无, 1=新, 2=热)
}
```

</details>

<details>
<summary><b>PlayListDetailDynamic</b> — 歌单动态数据</summary>

```rust
pub struct PlayListDetailDynamic {
    pub subscribed: bool,     // 是否已收藏
    pub booked_count: u64,    // 收藏数
    pub play_count: u64,      // 播放数
    pub comment_count: u64,   // 评论数
}
```

</details>

<details>
<summary><b>AlbumDetailDynamic</b> — 专辑动态数据</summary>

```rust
pub struct AlbumDetailDynamic {
    pub is_sub: bool,         // 是否已收藏
    pub sub_count: u64,       // 收藏数
    pub comment_count: u64,   // 评论数
}
```

</details>

---

## API 参考

### 认证 (Authentication)

#### `login`

```rust
pub async fn login(&self, username: &str, password: &str) -> Result<LoginInfo, NcmError>
```

账号密码登录，自动识别手机号/邮箱。

| 参数 | 类型 | 说明 |
|------|------|------|
| `username` | `&str` | 手机号（11位）或邮箱 |
| `password` | `&str` | 明文密码 |

<details>
<summary>响应类型 LoginInfo</summary>

```rust
pub struct LoginInfo {
    pub code: i32,           // 200=成功
    pub uid: u64,            // 用户 ID
    pub nickname: String,    // 昵称
    pub avatar_url: String,  // 头像 URL
    pub vip_type: i32,       // 0=无, 1=黑胶, 2=黑胶SVIP
    pub msg: String,         // 提示消息
}
```

</details>

---

#### `login_cellphone`

```rust
pub async fn login_cellphone(
    &self,
    ctcode: &str,
    phone: &str,
    captcha: &str,
) -> Result<LoginInfo, NcmError>
```

短信验证码登录。

| 参数 | 类型 | 说明 |
|------|------|------|
| `ctcode` | `&str` | 国际区号，如 `"86"` |
| `phone` | `&str` | 手机号 |
| `captcha` | `&str` | 短信验证码 |

<details>
<summary>响应类型 LoginInfo</summary>

```rust
pub struct LoginInfo {
    pub code: i32,
    pub uid: u64,
    pub nickname: String,
    pub avatar_url: String,
    pub vip_type: i32,
    pub msg: String,
}
```

</details>

---

#### `captcha`

```rust
pub async fn captcha(&self, ctcode: &str, phone: &str) -> Result<(), NcmError>
```

发送短信验证码。

| 参数 | 类型 | 说明 |
|------|------|------|
| `ctcode` | `&str` | 国际区号，如 `"86"` |
| `phone` | `&str` | 手机号 |

<details>
<summary>响应类型</summary>

`Result<(), NcmError>` — 无响应体，成功返回 `Ok(())`

</details>

---

#### `login_qr_create`

```rust
pub async fn login_qr_create(&self) -> Result<(String, String), NcmError>
```

创建二维码登录。返回 `(qr_url, unikey)`。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 (String, String)</summary>

- `qr_url` — 二维码图片 URL
- `unikey` — 用于轮询登录状态的唯一 key

</details>

---

#### `login_qr_check`

```rust
pub async fn login_qr_check(&self, key: &str) -> Result<Msg, NcmError>
```

轮询二维码扫码状态。

| 参数 | 类型 | 说明 |
|------|------|------|
| `key` | `&str` | `login_qr_create` 返回的 unikey |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg {
    pub code: i32,    // 200=已扫码, 803=已授权, 800=已过期
    pub msg: String,  // 状态描述
}
```

</details>

---

#### `login_status`

```rust
pub async fn login_status(&self) -> Result<LoginInfo, NcmError>
```

获取当前登录状态。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 LoginInfo</summary>

```rust
pub struct LoginInfo {
    pub code: i32,
    pub uid: u64,
    pub nickname: String,
    pub avatar_url: String,
    pub vip_type: i32,
    pub msg: String,
}
```

</details>

---

#### `logout`

```rust
pub async fn logout(&self) -> Result<Msg, NcmError>
```

退出登录。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg {
    pub code: i32,
    pub msg: String,
}
```

</details>

---

### 歌曲 (Song)

#### `songs_detail`

```rust
pub async fn songs_detail(&self, ids: &[u64]) -> Result<Vec<SongInfo>, NcmError>
```

批量获取歌曲详情。

| 参数 | 类型 | 说明 |
|------|------|------|
| `ids` | `&[u64]` | 歌曲 ID 列表 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: u64,
    pub pic_url: String,
    pub duration: u64,       // 毫秒
    pub copyright: SongCopyright,
}
```

</details>

---

#### `songs_url`

```rust
pub async fn songs_url(
    &self,
    ids: &[u64],
    br: &str,
) -> Result<Vec<SongUrl>, NcmError>
```

按码率获取歌曲播放地址（eapi 协议）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `ids` | `&[u64]` | 歌曲 ID 列表 |
| `br` | `&str` | 码率：`"128000"` / `"192000"` / `"320000"` / `"999000"` / `"1900000"` |

<details>
<summary>响应类型 Vec&lt;SongUrl&gt;</summary>

```rust
pub struct SongUrl {
    pub id: u64,
    pub url: String,          // 空字符串表示无版权
    pub rate: u32,            // 实际码率 (bps)
    pub quality: SongQuality,
}
```

</details>

---

#### `songs_url_v1`

```rust
pub async fn songs_url_v1(
    &self,
    ids: &[u64],
    level: SongQuality,
) -> Result<Vec<SongUrl>, NcmError>
```

按音质等级获取播放地址（v1 API，eapi 协议）。有损用 aac，无损用 flac。

| 参数 | 类型 | 说明 |
|------|------|------|
| `ids` | `&[u64]` | 歌曲 ID 列表 |
| `level` | `SongQuality` | 音质等级枚举 |

<details>
<summary>响应类型 Vec&lt;SongUrl&gt;</summary>

```rust
pub struct SongUrl {
    pub id: u64,
    pub url: String,
    pub rate: u32,
    pub quality: SongQuality,
}
```

</details>

---

#### `song_lyric`

```rust
pub async fn song_lyric(&self, id: u64) -> Result<Lyrics, NcmError>
```

获取歌曲歌词（原文 + 翻译）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌曲 ID |

<details>
<summary>响应类型 Lyrics</summary>

```rust
pub struct Lyrics {
    pub lyric: Vec<String>,    // 原文歌词，每行一条
    pub tlyric: Vec<String>,   // 翻译歌词（可能为空）
}
```

</details>

---

### 歌单 / 推荐 (Playlist)

#### `song_list_detail`

```rust
pub async fn song_list_detail(&self, id: u64) -> Result<PlayListDetail, NcmError>
```

获取歌单详情（含最多 1000 首歌曲）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌单 ID |

<details>
<summary>响应类型 PlayListDetail</summary>

```rust
pub struct PlayListDetail {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub description: String,
    pub create_time: u64,         // 毫秒时间戳
    pub track_update_time: u64,   // 毫秒时间戳
    pub songs: Vec<SongInfo>,
}
```

</details>

---

#### `liked_songs`

```rust
pub async fn liked_songs(&self, uid: u64) -> Result<Vec<SongInfo>, NcmError>
```

获取用户喜欢的歌曲（先取 ID 列表再批量查详情）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `uid` | `u64` | 用户 ID |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: u64,
    pub pic_url: String,
    pub duration: u64,
    pub copyright: SongCopyright,
}
```

</details>

---

#### `user_song_list`

```rust
pub async fn user_song_list(
    &self,
    uid: u64,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取用户创建/收藏的歌单列表。

| 参数 | 类型 | 说明 |
|------|------|------|
| `uid` | `u64` | 用户 ID |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub author: String,
}
```

</details>

---

#### `user_song_id_list`

```rust
pub async fn user_song_id_list(&self, uid: u64) -> Result<Vec<u64>, NcmError>
```

获取用户喜欢的歌曲 ID 列表（不含详情）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `uid` | `u64` | 用户 ID |

<details>
<summary>响应类型 Vec&lt;u64&gt;</summary>

纯 ID 列表，需配合 `songs_detail` 获取详情。

</details>

---

#### `songlist_detail_dynamic`

```rust
pub async fn songlist_detail_dynamic(&self, id: u64) -> Result<PlayListDetailDynamic, NcmError>
```

获取歌单动态数据（播放量、收藏数等）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌单 ID |

<details>
<summary>响应类型 PlayListDetailDynamic</summary>

```rust
pub struct PlayListDetailDynamic {
    pub subscribed: bool,     // 是否已收藏
    pub booked_count: u64,    // 收藏数
    pub play_count: u64,      // 播放数
    pub comment_count: u64,   // 评论数
}
```

</details>

---

#### `recommend_resource`

```rust
pub async fn recommend_resource(&self) -> Result<Vec<SongList>, NcmError>
```

获取每日推荐歌单。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub author: String,
}
```

</details>

---

#### `recommend_songs`

```rust
pub async fn recommend_songs(&self) -> Result<Vec<SongInfo>, NcmError>
```

获取每日推荐歌曲。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo {
    pub id: u64,
    pub name: String,
    pub singer: String,
    pub artist_id: u64,
    pub album: String,
    pub album_id: u64,
    pub pic_url: String,
    pub duration: u64,
    pub copyright: SongCopyright,
}
```

</details>

---

#### `simi_song`

```rust
pub async fn simi_song(
    &self,
    id: u64,
    limit: u16,
    offset: u16,
) -> Result<Vec<SongInfo>, NcmError>
```

获取与某首歌相似的歌曲（种子是歌曲本身，用于队列播完后的续播）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 种子歌曲 ID |
| `limit` | `u16` | 数量 |
| `offset` | `u16` | 偏移 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

#### `simi_playlist`

```rust
pub async fn simi_playlist(
    &self,
    id: u64,
    limit: u16,
    offset: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取包含某首歌的歌单。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌曲 ID |
| `limit` | `u16` | 数量 |
| `offset` | `u16` | 偏移 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

#### `personal_fm`

```rust
pub async fn personal_fm(&self) -> Result<Vec<SongInfo>, NcmError>
```

获取私人 FM 歌曲。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

### 搜索 (Search)

#### `search_song`

```rust
pub async fn search_song(
    &self,
    keyword: &str,
    offset: u16,
    limit: u16,
) -> Result<SearchResult, NcmError>
```

搜索歌曲。

| 参数 | 类型 | 说明 |
|------|------|------|
| `keyword` | `&str` | 搜索关键词 |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 SearchResult</summary>

```rust
pub struct SearchResult {
    pub songs: Vec<SongInfo>,
    pub total: u32,       // 总结果数
}
```

</details>

---

#### `search_songlist`

```rust
pub async fn search_songlist(
    &self,
    keyword: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

搜索歌单。

| 参数 | 类型 | 说明 |
|------|------|------|
| `keyword` | `&str` | 搜索关键词 |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub author: String,
}
```

</details>

---

#### `search_singer`

```rust
pub async fn search_singer(
    &self,
    keyword: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SingerInfo>, NcmError>
```

搜索歌手。

| 参数 | 类型 | 说明 |
|------|------|------|
| `keyword` | `&str` | 搜索关键词 |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SingerInfo&gt;</summary>

```rust
pub struct SingerInfo {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
}
```

</details>

---

#### `search_album`

```rust
pub async fn search_album(
    &self,
    keyword: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

搜索专辑。

| 参数 | 类型 | 说明 |
|------|------|------|
| `keyword` | `&str` | 搜索关键词 |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub author: String,
}
```

</details>

---

#### `search_lyrics`

```rust
pub async fn search_lyrics(
    &self,
    keyword: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongInfo>, NcmError>
```

按歌词内容搜索（搜索类型 1006）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `keyword` | `&str` | 歌词关键词 |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

#### `search_hot`

```rust
pub async fn search_hot(&self) -> Result<Vec<HotSearchItem>, NcmError>
```

获取热搜榜。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;HotSearchItem&gt;</summary>

```rust
pub struct HotSearchItem {
    pub keyword: String,  // 热搜关键词
    pub icon_type: i32,   // 0=无, 1=新, 2=热
}
```

</details>

---

### 排行榜 / 榜单 (Charts)

#### `toplist`

```rust
pub async fn toplist(&self) -> Result<Vec<TopList>, NcmError>
```

获取所有排行榜。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;TopList&gt;</summary>

```rust
pub struct TopList {
    pub id: u64,
    pub name: String,
    pub update: String,        // 更新频率描述
    pub description: String,
    pub cover: String,         // 封面图 URL
}
```

</details>

---

#### `top_songs`

```rust
pub async fn top_songs(&self, list_id: u64) -> Result<PlayListDetail, NcmError>
```

获取排行榜歌曲（等同 `song_list_detail`）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `list_id` | `u64` | 排行榜 ID，如 `19723756`（飙升榜） |

<details>
<summary>响应类型 PlayListDetail</summary>

```rust
pub struct PlayListDetail {
    pub id: u64,
    pub name: String,
    pub cover_img_url: String,
    pub description: String,
    pub create_time: u64,
    pub track_update_time: u64,
    pub songs: Vec<SongInfo>,
}
```

</details>

---

#### `top_song_list`

```rust
pub async fn top_song_list(
    &self,
    cat: &str,
    order: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

按分类浏览歌单。

| 参数 | 类型 | 说明 |
|------|------|------|
| `cat` | `&str` | 分类：`"全部"` / `"华语"` / `"流行"` 等 |
| `order` | `&str` | 排序：`"hot"` 或 `"new"` |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

#### `top_song_list_highquality`

```rust
pub async fn top_song_list_highquality(
    &self,
    cat: &str,
    lasttime: u64,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取精品歌单。

| 参数 | 类型 | 说明 |
|------|------|------|
| `cat` | `&str` | 分类 |
| `lasttime` | `u64` | 分页游标（上一页最后一条的 `trackUpdateTime`） |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

### 歌手 (Artist)

#### `singer_songs`

```rust
pub async fn singer_songs(&self, id: u64) -> Result<Vec<SongInfo>, NcmError>
```

获取歌手热门歌曲。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌手 ID |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

#### `singer_all_songs`

```rust
pub async fn singer_all_songs(
    &self,
    id: u64,
    order: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongInfo>, NcmError>
```

获取歌手全部歌曲（分页 + 排序）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌手 ID |
| `order` | `&str` | `"hot"`（热度）或 `"time"`（时间） |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

#### `top_artists`

```rust
pub async fn top_artists(
    &self,
    offset: u16,
    limit: u16,
) -> Result<Vec<SingerInfo>, NcmError>
```

获取热门歌手。

| 参数 | 类型 | 说明 |
|------|------|------|
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SingerInfo&gt;</summary>

```rust
pub struct SingerInfo { ... }
```

</details>

---

#### `toplist_artist`

```rust
pub async fn toplist_artist(&self, r#type: u8) -> Result<Vec<SingerInfo>, NcmError>
```

获取歌手排行榜（按地区）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `type` | `u8` | 地区：`1`=华语, `2`=欧美, `3`=韩国, `4`=日本 |

<details>
<summary>响应类型 Vec&lt;SingerInfo&gt;</summary>

固定返回 Top 100。

```rust
pub struct SingerInfo { ... }
```

</details>

---

### 专辑 (Album)

#### `simi_artist`

```rust
pub async fn simi_artist(&self, id: u64) -> Result<Vec<SingerInfo>, NcmError>
```

获取与某位歌手相似的歌手。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌手 ID |

<details>
<summary>响应类型 Vec&lt;SingerInfo&gt;</summary>

```rust
pub struct SingerInfo {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
}
```

</details>

---

#### `album`

```rust
pub async fn album(&self, album_id: u64) -> Result<AlbumDetail, NcmError>
```

获取专辑详情。

| 参数 | 类型 | 说明 |
|------|------|------|
| `album_id` | `u64` | 专辑 ID |

<details>
<summary>响应类型 AlbumDetail</summary>

```rust
pub struct AlbumDetail {
    pub id: u64,
    pub name: String,
    pub pic_url: String,
    pub description: String,
    pub publish_time: u64,       // 毫秒时间戳
    pub artist_id: u64,
    pub artist_name: String,
    pub artist_pic_url: String,
    pub songs: Vec<SongInfo>,
}
```

</details>

---

#### `album_sublist`

```rust
pub async fn album_sublist(
    &self,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取用户收藏的专辑。

| 参数 | 类型 | 说明 |
|------|------|------|
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

#### `album_detail_dynamic`

```rust
pub async fn album_detail_dynamic(&self, id: u64) -> Result<AlbumDetailDynamic, NcmError>
```

获取专辑动态数据。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 专辑 ID |

<details>
<summary>响应类型 AlbumDetailDynamic</summary>

```rust
pub struct AlbumDetailDynamic {
    pub is_sub: bool,
    pub sub_count: u64,
    pub comment_count: u64,
}
```

</details>

---

#### `new_albums`

```rust
pub async fn new_albums(
    &self,
    area: &str,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取新碟上架。

| 参数 | 类型 | 说明 |
|------|------|------|
| `area` | `&str` | 地区：`"ALL"` / `"ZH"` / `"EA"` / `"KR"` / `"JP"` |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

### MV (音乐视频)

MV 的入口是歌曲上的 `mv` 字段（`SongInfo::mv`，`0` 表示这首歌没有 MV；旧版搜索接口把同一个值放在 `mvid` 里），链子是 **歌曲 → `mv_detail` → `mv_url`**。

#### `mv_detail`

```rust
pub async fn mv_detail(&self, mv_id: u64) -> Result<MvInfo, NcmError>
```

获取 MV 详情（标题、海报、时长、歌手、发布时间、简介、各清晰度）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `mv_id` | `u64` | MV ID，来自 `SongInfo::mv` |

<details>
<summary>响应类型 MvInfo</summary>

```rust
pub struct MvInfo {
    pub id: u64,
    pub name: String,           // 标题
    pub artist_id: u64,         // 歌手 ID
    pub artist_name: String,    // 歌手名
    pub cover: String,          // 海报 URL
    pub duration: u64,          // 时长（毫秒）
    pub publish_time: String,   // 发布时间，形如 "1993-09-09"
    pub desc: String,           // 简介：服务端有 desc 就用 desc，否则用 briefDesc
    pub play_count: u64,
    pub sub_count: u64,
    pub share_count: u64,
    pub like_count: u64,        // /api/mv/detail 有，/api/v1/mv/detail 没有
    pub comment_count: u64,
    pub resolutions: Vec<MvResolution>,  // 该 MV 有的清晰度，升序
}
```

</details>

---

#### `mv_url`

```rust
pub async fn mv_url(
    &self,
    mv_id: u64,
    resolution: Option<u32>,
) -> Result<Vec<MvUrl>, NcmError>
```

获取 MV 的播放直链与画质元数据。请求打到 `/api/song/enhance/play/mv/url`（`/api/mv/url` 已下线，现役 Web 播放器用的就是前者，参数同样是 `id` / `r`）。一次只回一个清晰度，所以列表里通常只有一条；服务端会把请求的画质**降到该 MV 实际拥有的最好画质**（要 1080 可能给 480）。这个 MV 没有可播放画质时返回 `NcmError::Message("这个 MV 没有可播放的清晰度")`。

| 参数 | 类型 | 说明 |
|------|------|------|
| `mv_id` | `u64` | MV ID |
| `resolution` | `Option<u32>` | 期望的竖向分辨率：`240` / `480` / `720` / `1080`；`None` 时按 `1080` 请求 |

<details>
<summary>响应类型 Vec&lt;MvUrl&gt;</summary>

```rust
pub struct MvUrl {
    pub id: u64,
    pub url: String,          // 直链，带 wsSecret / wsTime，有时效
    pub resolution: u32,      // 实际画质（服务端可能降级）
    pub size: u64,            // 字节数
    pub expire_secs: u64,     // 直链有效期（秒）
}
```

</details>

---

### 用户交互 (Interaction)

#### `like`

```rust
pub async fn like(&self, song_id: u64, like: bool) -> Result<Msg, NcmError>
```

喜欢 / 取消喜欢歌曲。

| 参数 | 类型 | 说明 |
|------|------|------|
| `song_id` | `u64` | 歌曲 ID |
| `like` | `bool` | `true`=喜欢, `false`=取消 |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg {
    pub code: i32,
    pub msg: String,
}
```

</details>

---

#### `fm_trash`

```rust
pub async fn fm_trash(&self, song_id: u64) -> Result<Msg, NcmError>
```

将歌曲标记为私人 FM 不喜欢。

| 参数 | 类型 | 说明 |
|------|------|------|
| `song_id` | `u64` | 歌曲 ID |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg { ... }
```

</details>

---

#### `song_list_like`

```rust
pub async fn song_list_like(
    &self,
    id: u64,
    like: bool,
) -> Result<Msg, NcmError>
```

收藏 / 取消收藏歌单。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 歌单 ID |
| `like` | `bool` | `true`=收藏, `false`=取消 |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg { ... }
```

</details>

---

#### `album_like`

```rust
pub async fn album_like(&self, id: u64, like: bool) -> Result<Msg, NcmError>
```

收藏 / 取消收藏专辑。

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `u64` | 专辑 ID |
| `like` | `bool` | `true`=收藏, `false`=取消 |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg { ... }
```

</details>

---

### 每日任务 / 播放上报 (Daily)

#### `daily_task`

```rust
pub async fn daily_task(&self, r#type: &str) -> Result<Msg, NcmError>
```

每日签到。

| 参数 | 类型 | 说明 |
|------|------|------|
| `type` | `&str` | `"0"`=PC端, `"1"`=移动端 |

<details>
<summary>响应类型 Msg</summary>

```rust
pub struct Msg { ... }
```

</details>

---

#### `report_play`

```rust
pub async fn report_play(
    &self,
    song_id: u64,
    time_ms: u64,
    source_id: Option<u64>,
) -> Result<(), NcmError>
```

上报播放记录（听歌打卡 / 播放量贡献）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `song_id` | `u64` | 歌曲 ID |
| `time_ms` | `u64` | 播放时长（毫秒） |
| `source_id` | `Option<u64>` | 来源歌单 ID（可选） |

<details>
<summary>响应类型</summary>

`Result<(), NcmError>` — 忽略响应体

</details>

---

### 用户数据 (User Data)

#### `recent_songs`

```rust
pub async fn recent_songs(&self, limit: u16) -> Result<Vec<SongInfo>, NcmError>
```

获取最近播放的歌曲。

| 参数 | 类型 | 说明 |
|------|------|------|
| `limit` | `u16` | 返回数量 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

#### `user_record`

```rust
pub async fn user_record(&self, uid: u64, week: bool) -> Result<Vec<PlayRecord>, NcmError>
```

获取听歌排行（带播放次数与分数）。`recent_songs` 只有「最近听了什么」的列表，
这一条才是口味画像的输入。

| 参数 | 类型 | 说明 |
|------|------|------|
| `uid` | `u64` | 用户 ID |
| `week` | `bool` | `true` 取近一周，`false` 取全部时间 |

<details>
<summary>响应类型 Vec&lt;PlayRecord&gt;</summary>

```rust
pub struct PlayRecord {
    pub song: SongInfo,
    pub play_count: u64,
    pub score: u64,
}
```

</details>

---

#### `user_cloud_disk`

```rust
pub async fn user_cloud_disk(&self) -> Result<Vec<SongInfo>, NcmError>
```

获取用户云盘歌曲（固定取全部，limit=10000）。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

### 电台 / 播客 (Radio)

#### `user_radio_sublist`

```rust
pub async fn user_radio_sublist(
    &self,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongList>, NcmError>
```

获取用户订阅的电台。

| 参数 | 类型 | 说明 |
|------|------|------|
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

#### `djradio_recommend`

```rust
pub async fn djradio_recommend(&self) -> Result<Vec<SongList>, NcmError>
```

获取推荐电台（区别于已订阅列表：还没订阅任何电台时看到的就是这一份）。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;SongList&gt;</summary>

```rust
pub struct SongList { ... }
```

</details>

---

#### `radio_program`

```rust
pub async fn radio_program(
    &self,
    rid: u64,
    offset: u16,
    limit: u16,
) -> Result<Vec<SongInfo>, NcmError>
```

获取电台节目列表。

| 参数 | 类型 | 说明 |
|------|------|------|
| `rid` | `u64` | 电台 ID |
| `offset` | `u16` | 分页偏移 |
| `limit` | `u16` | 每页数量 |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

节目以 `SongInfo` 形式返回。

</details>

---

### 智能播放 (Intelligence)

#### `playmode_intelligence_list`

```rust
pub async fn playmode_intelligence_list(
    &self,
    song_id: u64,
    playlist_id: u64,
) -> Result<Vec<SongInfo>, NcmError>
```

获取心动模式 / 智能推荐歌曲列表（基于种子歌曲的 AI 推荐）。

| 参数 | 类型 | 说明 |
|------|------|------|
| `song_id` | `u64` | 当前播放歌曲 ID |
| `playlist_id` | `u64` | 来源歌单 ID |

<details>
<summary>响应类型 Vec&lt;SongInfo&gt;</summary>

```rust
pub struct SongInfo { ... }
```

</details>

---

### 首页 / 发现 (Homepage)

#### `banners`

```rust
pub async fn banners(&self) -> Result<Vec<BannersInfo>, NcmError>
```

获取首页轮播图。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型 Vec&lt;BannersInfo&gt;</summary>

```rust
pub struct BannersInfo {
    pub pic: String,             // 图片 URL
    pub target_id: u64,          // 关联目标 ID
    pub target_type: TargetType, // Song=1, Album=10
}
```

</details>

---

#### `homepage`

```rust
pub async fn homepage(&self) -> Result<String, NcmError>
```

获取 APP 首页推荐数据，返回原始 JSON 字符串。

| 参数 | 无 |
|------|-----|

<details>
<summary>响应类型</summary>

`Result<String, NcmError>` — 未解析的原始 JSON

</details>

---

### 下载 (Download)

#### `download_img`

```rust
pub async fn download_img(
    &self,
    url: &str,
    path: std::path::PathBuf,
    width: u16,
    height: u16,
) -> Result<(), NcmError>
```

下载图片到本地。文件已存在则跳过。URL 会自动追加 `?param={width}y{height}`。

| 参数 | 类型 | 说明 |
|------|------|------|
| `url` | `&str` | 图片 URL |
| `path` | `PathBuf` | 本地保存路径（含文件名） |
| `width` | `u16` | 目标宽度 |
| `height` | `u16` | 目标高度 |

<details>
<summary>响应类型</summary>

`Result<(), NcmError>` — 无响应体，成功返回 `Ok(())`

</details>

---

#### `download_song`

```rust
pub async fn download_song(
    &self,
    url: &str,
    path: std::path::PathBuf,
) -> Result<(), NcmError>
```

下载歌曲文件到本地。文件已存在则跳过。

| 参数 | 类型 | 说明 |
|------|------|------|
| `url` | `&str` | 歌曲文件 URL |
| `path` | `PathBuf` | 本地保存路径（含文件名） |

<details>
<summary>响应类型</summary>

`Result<(), NcmError>` — 无响应体，成功返回 `Ok(())`

</details>

---

### 同步方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `builder` | `pub fn builder() -> NcmClientBuilder` | 获取默认构建器 |
| `new` | `pub fn new() -> Result<Self, NcmError>` | 用默认配置创建客户端 |
| `flush_cookies` | `pub fn flush_cookies(&self)` | 手动将 Cookie 持久化到磁盘 |
| `is_logged_in` | `pub fn is_logged_in(&self) -> bool` | 通过 `MUSIC_U` / `__csrf` Cookie 检查登录状态 |
| `cookie_store` | `pub fn cookie_store(&self) -> &Arc<Mutex<CookieStore>>` | 访问内部 CookieStore |
