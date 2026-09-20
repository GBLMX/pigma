## [Unreleased]

### 🚀 Features

- *(ui)* **登录页成为唯一的认证入口**：二维码 / 账号密码 / 短信验证码 / 每日签到四种方式都在登录页里，方法之间与表单字段之间可切换，密码字段掩码显示；`:login` / `:signin` / `:sms` / `:smslogin` / `:sign` 仍可用，但改为**跳到登录页并选中对应方法**并把参数预填进表单 —— 四种方式只有一份实现
- *(auth)* 短信验证码的「发送」与「登录」两步**共用同一个手机号**（此前 `:sms` 只提示用户自己再输一遍 `:smslogin <手机号> <验证码>`，输错号即失败）

### 🐛 Bug Fixes

- *(auth)* **每日签到的结果此前常常是空提示**：`dailyTask` 成功时响应里只有 `point` 没有 `msg`，而回显直接取 `msg`，于是弹出一片空白。现在把它变成成句中文（`签到成功（云贝 N）` / `今天已签到`），业务失败则返回真正的错误并带上服务端 code 与原因
- *(auth)* **风控错误丢失了服务端的说明，也没给出路**：`parse_login_info` 只读 `msg` 而风控响应把原因放在 `message` 里，于是 `-460` 显示成无用的 `登录失败 (code=-460)`。现在两种字段都读；`-460` / `-462` 直接给出可行建议 —— **终端里做不了网易云的滑块验证，请改用二维码登录**（这是当前唯一确定可用的登录方式）
- *(auth)* **页面上的登录错误现在是一行干净的句子**：`NcmError::Parse` 的 Display 是给日志用的多行文本（`parse: …` + 原始响应体），而登录页的错误区只有一行 —— 于是既出现重复的「登录失败: 登录失败:」，又被截断，`-460`/`-462` 的提示与出路会直接被切掉。现在这类「文案本身就是给用户看的」错误用新的 `NcmError::Message` 变体原样显示，不再套内部包装名与响应体
- *(auth)* `:sign` 未登录时此前显示 `API code=301: unknown error`（服务端没有 `msg` 字段）；现在直接说「需要登录后才能签到」
- *(auth)* 登录成功后界面可能完全不刷新：此前只在「当前在登录页 / Splash」或「原本未登录且当前在主页」时才重新加载，**在排行榜、搜索页等其它页面用 `:signin` 登录成功后，页面看起来仍是未登录**
## [1.1.0] - 2026-09-21

上游 `## Features` 里剩下的四项待办一次做完（`landing page` 不做）。

### 🚀 Features

- *(local)* **本地音频读真实标签**：标题 / 歌手 / 专辑 / 时长改用 `lofty` 读取（`read_cover_art(false)`，全库扫描不解析内嵌封面）。取不到标签时行为与本版之前**完全一致**：标题退回文件名、歌手退回「本地」、时长退回 rodio 解码、专辑退回文件路径。本地曲目的**歌手取值顺序与云盘上传一致**（TrackArtist → AlbumArtist）
- *(local)* **本地歌词**：识别音频旁的**同名侧车 `.lrc`**（大小写不敏感 ✓ 去 BOM ✓ 无有效时间戳则不冒充歌词 ✓），复用既有的 `parse_lyric_lines` 与歌词状态管线，不另造第二套。**故意不做缓存**：读本地文件与读它的缓存成本相当，而用户改了 `.lrc` 应当下次播放就生效
- *(cli/tui)* **更多可运行时修改的配置**：`:notify song_change|errors on|off`、`:mouse on|off`、`:cursor default|block|underline|bar`、`:lyricgradient <预设>`、`:saveonplay on|off`。终端类（鼠标捕获、光标形状）直接写对应的转义序列**当场生效**，其余下一帧/下一首生效；全部立即写回 `config.toml` 并进入 `ctrl+p` 命令面板
- *(playback)* **云盘作为兜底源**：解析链变成 NCM（网络错误重试一次）→ sonar 兜底 → **云盘兜底** → 才报错；云盘用的是既有的 `/weapi/v1/cloud/get` 分页 + 本地匹配（标题去括号段、忽略大小写；歌手互相包含；时长差 ≤5s 优先）。未登录 / 未命中 / 接口报错一律**安静回退**并保留原错误
- *(ui)* **歌手详情页**：进入方式与既有表格一致（`Enter` 进入歌手），页面显示简介 / 热门曲目 / 专辑，支持 `j k g G`、`Esc` 返回、`Enter` 播放、`r` 重载、鼠标滚动与双击

### 🐛 Bug Fixes

- *(core)* 新增 `AppEvent::Repaint`：此前主循环**只在收到事件时重绘**，没有歌在播时会阻塞在事件流上，于是后台任务填好的状态（如歌手页加载完成）要等到用户按任意键才上屏 —— 画面会一直停在「正在加载」
- *(playback)* 云盘匹配对 `(Remastered 2011)` 这类括号后缀命中不了：改为先去掉括号段再比较（由一次性探针抓出）

### 💼 Other

- `SongInfo` 新增 `local_path: Option<String>`：路径此前被塞在 `album` 里，导致本地播放与云盘上传都从专辑字段取文件、专辑标签无处安放。拆开后播放/上传读路径、专辑字段放标签（缺失退回路径），并补了 2 条契约测试（网络歌曲不多出该键；设过的路径必须能往返）
- 本地音乐目录仍是硬编码的 `~/Music`（`app/navigation.rs` 与 `service.rs` 两处），本次未改为配置项

## [1.0.1] - 2026-09-21

### 🐛 Bug Fixes

- *(cli)* **`boxpigma msg search <关键词> | head` 会 core dump**：Rust 启动时忽略 `SIGPIPE`，于是向已关闭的管道写入会变成写错误，`println!` 又把写错误变成 panic —— 发布档的 `panic = "abort"` 再把它变成 `SIGABRT` 与一份 core dump（退出码 134），而别的 Unix 工具只会安静退出。现在**子命令在做任何事之前**恢复 `SIGPIPE` 的默认行为（退出码 141、stderr 干净）；**TUI 与守护进程刻意不动** —— 它们必须活着才能还原终端，也不往管道里写。**这不是 1.0.0 引入的**：0.2.14 的发布二进制表现完全相同（已用其自带守护进程对照验证）

### 📚 Documentation

- 记录**内存占用实测**（同机、读 `/proc/<pid>/status` 的 `VmRSS`、0.2 秒粒度采样 24 秒）：TUI 主界面 **14.6 MB**、守护进程 **13.7 MB**，对照旧版 14.0 MB / 13.4 MB；四种情况的**启动峰值都等于 20 秒后的稳态**，且 **RSS ≈ 二进制体积 + 约 3 MB**
- 新增 [`FRAMEWORK.md`](./FRAMEWORK.md)：每项框架调整的改前改后、实测依据与代价，以及**被否决的候选**（`unicode-truncate` 不等价、`nucleo` 是功能变更、`palette` 无终端量化、`taskdump` 在本机会卡住等）
- README 全面重写，围绕 1.0 的改名与迁移、框架调整与实测数字

## [1.0.0] - 2026-09-21

### ⚠️ 破坏性变更

- **改名 `pigma` → `boxpigma`**：包名、二进制名、配置目录（`~/.config/boxpigma`）、缓存目录（`~/.cache/boxpigma`）、IPC socket（`pigma.sock` → `boxpigma.sock`）与 Windows 命名管道（`\\.\pipe\boxpigma`）全部随之改名
- **升级不会丢数据**：首次运行会把旧的 `pigma` 目录整体搬到新名字下（配置、cookie、队列、封面缓存都在里面），日志里留一行 `adopted … from before the rename`；若搬不动（比如旧实例还在跑、权限不足）则继续使用旧目录，绝不静默从空配置开始
- 旧二进制与**正在运行的旧守护进程**不会自动升级：请重启守护进程；`~/.local/bin/pigma` 是旧文件，可自行删除
- AUR 包名一并改为 `boxpigma-gblmx-bin`（发布到 AUR 需要以新包名注册；若希望保留旧包名，改回 `PKGBUILD` 与 release 工作流里的两处即可）

### 🚀 Features

- *(theme)* `default_theme = "random"`（以及 `light_theme`）每次启动随机挑一个主题；`:theme random` 立刻重掷并把抽到的那一个写回配置；补全与命令面板都能选到 `random`（它排在主题列表最前，因此循环切换每圈遇到一次）
- *(log)* 日志改用 `tracing` + `tracing-appender`：按天轮转、保留最近 7 个，行内带模块路径与本地时间。此前是单个无限增长的文件（实测已 2.8 MB），且写入在调用线程持锁同步进行；**135 个 `log::*!` 调用点一行未改**，由 `tracing-log` 桥接
- *(notify)* kitty 终端改用其自有的 `OSC 99` 通知：标题与正文分开、Base64 负载（`e=1`）、并用 `f=` 声明应用名，便于用户过滤；其余终端保持 `OSC 9` 不变
- *(ui)* 键位表里的页面行由页面表生成，页面不可能再从 `?` 里漏掉

### 🐛 Bug Fixes

- *(theme)* `all_names()` 取自 `HashMap`，`:theme` 的循环顺序每次启动都不同 —— 现在排序，顺序稳定
- *(net)* sonar 搜索、封面下载、音频流三处的 reqwest 客户端此前**没有任何超时**；现在统一 `connect_timeout` 10s + `read_timeout` 30s，封面与搜索另加 30s 总时限。音频流**刻意不加总超时**（会截断正在播放的下载），已用 34 秒持续下载的探针验证不会被切断

### 💼 Other

- *(ui)* 页面分发、页面按键与键位表合并为一张表：新增一个页面从改 **9 个文件降到 2 个**（`ui.rs` 生产代码里的页面匹配臂 5 → 0，`layout.rs` 3 → 0）
- *(ui)* 铺底色改用 `Fill`，不再用一个无边界的 `Block`
- *(theme)* WCAG 亮度/对比度改用 `palette`，全仓颜色数学从三处收敛到一处；等价性用全部 2²⁴ 个 8 位 sRGB 颜色与 19 个内置主题逐一核对（`on_accent` 结果全部一致）
- *(cli)* 版本号升至 `1.0.0`

## [0.2.14] - 2026-09-12

### 🚀 Features

- *(app)* Add login status request guard and improve song management in navigation (akirco)

### 🐛 Bug Fixes

- Restore terminal state and sync startup auth (caojialin)
- Synchronize liked songs pagination (Mars160)
- 修复帮助面板滚动溢出导致向上滚动失效的问题 (lorlike)

### 💼 Other

- *(deps)* Bump log from 0.4.33 to 0.4.34 (dependabot[bot])
- *(deps)* Bump stream-download from 0.24.3 to 0.24.4 (dependabot[bot])

### 📚 Documentation

- *(README)* Enhance completion script instructions for bash, zsh, fish, and powershell (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Fmt (akirco)
## [0.2.13] - 2026-08-22

### 🚀 Features


- *(cli/waybar)* Remove native waybar output (`--waybar`); waybar integration now uses a standalone bash script (`waybar/pigma`) that calls `pigma status --json` per invocation, with `ensure_daemon` for auto-start
- *(cli)* Add `pigma msg play <song-id>` to jump to a song in the queue by id, and `pigma msg toggle_play` play/pause toggle (akirco)
- *(cli)* Add `pigma msg search <keyword>`: the daemon searches NCM + sonar sources, returns songs tagged by source, and registers them so `pigma msg play <id>` can enqueue and play a search result across instances (sonar synthetic ids resolve in-process) (akirco)
- *(cli)* Move queue listing into `pigma msg list` (reusing the TUI queue-table rendering, `▶` marks the current song); drop the standalone `pigma list <endpoint>` command so the CLI only talks to a running instance (akirco)
- *(cli)* Add `pigma completions <shell>` to generate shell completion scripts (bash/zsh/fish/elvish/powershell); `msg` actions are now a clap `ValueEnum`, so `pigma msg <Tab>` completes them (with aliases) (akirco)
- *(cli)* Fold the global `--playlist` into `-d` via `ENDPOINT:N` (e.g. `pigma -d toplist:3`); the old global flag stays as a hidden backward-compatible alias. `msg switch-list --playlist N` is unchanged (akirco)


### 🐛 Bug Fixes

- *(playback)* Rebuild the audio device when the output stream dies (#61): Bluetooth disconnect/reconnect no longer leaves playback silent. The sink is now owned by the player task; fatal cpal stream errors (`DeviceNotAvailable`/`StreamInvalidated`/backend device loss on WASAPI/CoreAudio/ALSA) and a frozen-position watchdog (suppressed during recent network underruns) both trigger a rebuild that resumes at the last position. The player also follows system default-output changes (debounced, stable-id based), so after a Bluetooth headset reconnects playback moves back to it automatically — macOS, Windows and Linux alike


## [0.2.12] - 2026-08-16

### 🚀 Features

- *(path)* Add expand_tilde function to handle home directory paths (akirco)

### 🐛 Bug Fixes

- *(cli)* List download and local music panic (akirco)

### 🚜 Refactor

- *(cli)* Streamline CLI command handling and improve structure (akirco)
- *(bilivideo)* Improve cookie handling and enhance error logging for transient failures (akirco)
## [0.2.11] - 2026-08-16

### 🐛 Bug Fixes

- *(playback)* Bundle minimal ALSA config for musl static builds (akirco)
- *(playback)* Use default hw card in bundled musl ALSA config (akirco)

### 💼 Other

- Remove musl targets (static musl cannot use PipeWire/PA plugins) (akirco)

### 🎨 Styling

- *(playback)* Fmt and wrap unsafe set_var for edition 2024 (akirco)

### ⚙️ Miscellaneous Tasks

- Generate release notes with git-cliff and remove release-drafter (akirco)
- *(ci)* Remove 'dev' branch from CI trigger paths (akirco)
# Changelog

## [0.2.10] - 2026-08-16

### 🚀 Features

- *(ci)* Build linux aarch64 and musl targets with cross in CI and release (akirco)
- Add initial configuration for TOML format rules (akirco)

### 🐛 Bug Fixes

- *(manifest)* Flatten multi-line inline tables to single-line for strict TOML parsers (akirco)
- Update JSON structure for playback control commands in documentation (akirco)

### 🚜 Refactor

- Refactor cache management and improve performance (akirco)

### ⚙️ Miscellaneous Tasks

- Trigger CI on dev branch pushes (akirco)
## [0.2.9] - 2026-08-15

### 🚀 Features

- Add IPC support for pigma status and msg commands (akirco)
- *(waybar)* Add configuration and scripts for Pigma integration (akirco)
- Enhance IPC with queue management and headless mode support (akirco)
- *(ipc)* Add Windows named pipe support and update IPC documentation (akirco)
- *(ci)* Add aarch64 and aarch64-musl build jobs to CI workflow (akirco)
- *(ci)* Remove Linux audio dependencies installation and add pre-build scripts for aarch64 targets (akirco)
- *(ci)* Replace build script with inline pre-build steps for aarch64-musl target (akirco)
- *(ci)* Add x86_64-musl build job and update pre-build steps for ALSA (akirco)

### 🐛 Bug Fixes

- *(search)* Clamp search limit to prevent exceeding API constraints (akirco)
- *(ci)* Update ALSA_URL to point to the official ALSA project site (akirco)
- *(ci)* Update pre-build steps and environment variables for aarch64 and x86_64 targets (akirco)
- *(ci)* Update ALSA version and add environment variables for cross-compilation (akirco)
- *(ci)* Streamline environment variable setup and remove unnecessary build.env section (akirco)
- *(playback)* Update cfg attributes for Linux to include GNU environment (akirco)
- *(ci)* Add 'sed' to install dependencies for ALSA build (akirco)
- *(playback)* Refine Linux target configuration to include GNU environment (akirco)
- *(ci)* Add installation of musl-tools and ALSA build steps (akirco)
- *(ci)* Update musl-tools installation and setup for ALSA build (akirco)
- *(ci)* Enhance musl-tools installation for ALSA build with additional flags (akirco)
- *(ci)* Add symlink for asm-generic in musl-tools installation (akirco)

### 🚜 Refactor

- *(sonar)* Update dependencies and improve MD5 usage (akirco)

### ⚙️ Miscellaneous Tasks

- Add VSCode settings for CSS file association (akirco)
- *(ci)* Fmt (akirco)
## [0.2.8] - 2026-08-13

### 🚀 Features

- Enhance theme configuration and navigation features (akirco)
- Updated default theme colors for better visibility and aesthetics.
- Added navigation event handling for login functionality in input handling.
- Improved login key handling to navigate to the main page upon escape.
- Enhanced main input handling with volume adjustment and navigation position cycling.
- Refined splash screen input handling to streamline user experience.
- Modified layout structures to accommodate new logo rendering in the login screen.
- Implemented API service checks for login requirements on specific endpoints.
- Introduced command actions for cycling navigation positions.
- Enhanced splash state to track display duration.
- Updated UI components for better rendering and user feedback.
- Added help text for new login functionality and volume controls.
- Improved overall code structure and readability across multiple files.

### 🐛 Bug Fixes

- *(scan)* Prevent ID collision with sonar song-id by clearing top bit (akirco)
- *(splash)* Update version display to use dynamic package version (akirco)

### 🚜 Refactor

- *(ui)* Replace create_block with CornerBlock for improved block handling (akirco)
- *(README)* Update feature list and improve markup syntax explanation (akirco)
- Refactor navigation state and UI components for improved clarity and functionality
- Updated NavState to include methods for retrieving focused section, selected index, and selected item.
- Removed unused LoginState from NavigationState and adjusted related references.
- Enhanced navigation drawing logic to utilize new NavState methods for better readability.
- Added documentation comments to clarify the purpose of various functions and structures.
- Refined text input and UI rendering components with improved comments and organization.
- Implemented a new render_gauge function to streamline gauge rendering in the player bar.
- Updated utility functions for better clarity and consistency in naming conventions.
- Enhanced gradient and path utilities with improved documentation for better understanding.

### ⚙️ Miscellaneous Tasks

- *(ncm_client)* Fmt (akirco)

### 📚 Documentation

- *(README)* add splash screen and plan sections to improve documentation clarity
- *(config)* document splash duration setting in configuration example

## [0.2.7] - 2026-08-11

### 🚀 Features

- Add Homebrew tap update workflow for tag releases (akirco)
- Enhance audio playback error handling and buffering logic (akirco)

### 🚜 Refactor

- Remove ApiEndpoint enum from api.rs and integrate it into service.rs (akirco)
- *(app)* Implement event handling and navigation improvements (akirco)

### ⚙️ Miscellaneous Tasks

- Update dependencies in Cargo.lock and Cargo.toml for ncm-api and sonar (akirco)
## [0.2.6] - 2026-08-10

### 🚜 Refactor

- Update cookie file handling to improve permissions and writing logic (akirco)
- Refactor playback and UI components for improved performance and clarity
- Updated `handle_main_key` to remove unnecessary Arc wrapping for songs.
- Introduced `current_resolve` in `PlaybackEngine` to manage in-flight song resolve tasks.
- Modified `activate_by_id` to accept a `persist_previous` flag for better queue management.
- Changed song handling in `play_songs` and `append_songs_to_key` to use `Arc<SongInfo>` directly.
- Enhanced `ApiService` to return `Arc<SongInfo>` for shared ownership across components.
- Refactored UI rendering functions to streamline theme resolution and scrollbar rendering.
- Updated help text for clearing the playback queue to use 'w' instead of 'Ctrl+L'.
- Improved overall code readability and maintainability by reducing unnecessary clones and enhancing comments.

## [0.2.5] - 2026-08-09

### 🚀 Features

- Add keyboard shortcuts for manual refresh and help panel (akirco)
- Enhance song liking functionality with improved event handling and user feedback (akirco)
- Implement liked songs functionality with cloud sync and UI updates (akirco)

### 🐛 Bug Fixes

- *(ci)* Fmt (akirco)
## [0.2.4] - 2026-08-09

### 🚀 Features

- Add 'save_on_play' configuration and related functionality (akirco)
- Add Skeleton widget for loading state representation (akirco)

### 🚜 Refactor

- *(ncm-api)* Rewrite ncm-api (akirco)
- Update imports to use playback module for PlaybackState (akirco)
## [0.2.3] - 2026-08-08

### 🐛 Bug Fixes

- *(cover)* Wt img capability check (akirco)

### ⚙️ Miscellaneous Tasks

- Adjust file and code structure (akirco)
- Update config example (akirco)
- *(ci)* Fmt (akirco)
## [0.2.2] - 2026-08-07

### 🚀 Features

- *(sonar)* New fetch playlist api(get track_ids + lazy pagination) (akirco)
- *(mode)* Add toast for mode switch (akirco)

### 🐛 Bug Fixes

- *(sonar)* Examples and new test for bibili search (akirco)
- *(styled_text)* Styled_text is overrideded by default (akirco)

### 🚜 Refactor

- *(content)* Use built-in Row instead of manually (akirco)
- *(playlist)* New data loading logic (akirco)
- *(utils)* Remove unnecessary utils export (akirco)
- *(playerbar)* Simplify code logic (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Fmt (akirco)
## [0.2.1] - 2026-08-06

### 🐛 Bug Fixes

- *(theme)* Unknown color name(removed) (akirco)
## [0.2.0] - 2026-08-06

### 🚀 Features

- *(musicx)* Third-party multi-source search with lyrics/cover fallback (akirco)
- *(musicx)* Lyrics/cover loading and playback queue integration (akirco)
- *(input)* Search source switching and shortcut enhancements (Tab to switch source, g/G, S to like current playing) (akirco)
- *(ui)* Help popup and proxy config support for Normal/Reversed/Both (akirco)
- *(layout)* Hide sidebar and adapt cover size on narrow terminals (akirco)
- *(theme)* Add light theme and title style (akirco)
- *(playback)* Update progress bar color on cache completion and persist musicx registry (akirco)
- *(musicx)* Register utils::musicx module (akirco)

### 🐛 Bug Fixes

- *(playback)* Bilibili stream download 403 and proxy support for stream downloads (akirco)
- *(ncm-api)* Use device id and md5-hashed password in login (akirco)

### 🚜 Refactor

- *(playback)* Remove types.rs, merge types into playback.rs (akirco)
- [**breaking**] Rename musicx crate to sonar (akirco)
- *(core)* Migrate pigma to the sonar crate (akirco)

### 📚 Documentation

- Update README.md (akirco)

### 🧪 Testing

- *(musicx)* Testing (akirco)

### ⚙️ Miscellaneous Tasks

- *(ci)* Bump checkout and ssh-agent actions (akirco)
- *(config)* Update example config and linker flags (akirco)
## [0.1.9] - 2026-08-03

### 🚀 Features

- *(crates/musicx)* Unifield fallback sound source (akirco)

### 🚜 Refactor

- *(config)* Restructuring the config structure (akirco)
- *(fallback)* Using new sound source fallback (akirco)
- *(layout)* New navigation layout (akirco)

### 📚 Documentation

- *(README)* Add aur installtion desc (akirco)
- Update README.md (akirco)

### ⚙️ Miscellaneous Tasks

- Update deps (akirco)
## [0.1.8] - 2026-08-01

### 🚀 Features

- *(navigation)* New layout (top) (akirco)

### ⚙️ Miscellaneous Tasks

- *(release)* Add aur release (akirco)
- *(state)* Nav.rs renamed to navigation.rs (akirco)
## [0.1.7] - 2026-07-30

### 🐛 Bug Fixes

- *(events)* Seeking spinner (akirco)
- *(cover)* Ratatui-image image protocol check failed (akirco)
- *(navigation)* Need not cache failed responses (akirco)

### 🚜 Refactor

- *(playback)* Optimize memory by reusing player & manual memory recycling (akirco)

## [0.1.6] - 2026-07-29

### 💼 Other

- *(deps)* Bump actions/checkout from 4 to 7 (dependabot[bot])
- *(deps)* Bump softprops/action-gh-release from 2 to 3 (dependabot[bot])
- *(deps)* Bump actions/upload-artifact from 4 to 7 (dependabot[bot])

### 🚜 Refactor

- *(config)* Rewrite config file inline ArrayOfTables. (akirco)
- *(playerbar)* Fix layout issues (akirco)
- *(playerback)* Adjust cpal buffersize,reduce the frequency of thread wakes (akirco)
## [0.1.5] - 2026-07-27

### 🚀 Features

- *(navigation)* Add saved albums navigation tab (#18) (AshGrey🥕)
- Migrate API to service calls, improve uploads, local music cloud drive, and cover caching, update contribution guide (akirco)
- fixs: cache value `accessed_at` always 0, table content overwritten during fast navigation (akirco)

## [0.1.4] - 2026-07-26

### Added

- Daily recommendation "not interested": press `d` to mark a song as not interested, telling the algorithm not to recommend similar songs
- Daily recommendation "like": press `s` to add a song to My Liked Music (available on all song pages)
- Proxy target config `proxy_target`: supports `yt` (proxy YouTube, default), `ncm` (proxy NetEase Cloud Music), `both` (proxy both)

### Changed

- Perf: per-character gradient lyric rendering eliminates per-char String allocation (zero-allocation borrowing)
- Perf: gradient preset changed from string dispatch to enum match, eliminating multiple string comparisons per frame
- Perf: table field query returns `Cow` to avoid String clone
- Perf: player bar time display reuses `format_duration_into` buffer
- Perf: cache lookup merged into a single RwLock + iteration (was 4 locks + stat)
- Perf: cache total size tracked via `AtomicU64`, evict avoids O(n) stat syscalls
- Perf: evict sorting avoids filename clone
- Perf: `collect_cached_songs` removes redundant `path.exists()` check
- Perf: storage IO (playlist saving) offloaded to blocking thread via `spawn_blocking`
- Local music now loads on demand: released when switching navigation, reloaded from disk cache or re-scanned when returning
- My Liked Music: fixed missing cache write, now writes to disk cache after first load
- My Liked Music: most recently liked songs shown at top of list (IDs reversed)
- NCM proxy fix: corrected `like` endpoint params (endpoint `/api/radio/like`, params `trackId`/`alg`/`time`)
- Daily recommendation dislike endpoint corrected to `/api/v2/discovery/recommend/dislike` (params `resId`/`resType`/`sceneType`)

### Removed

- Playback reporting feature (`report_play` API call and `pending_report` mechanism)

### Fixed

- Fixed ratatui-image loading album covers using excessive memory (request 200x200 thumbnail from NCM CDN instead of original image)

## [0.1.3] - 2026-07-25

### Added

- Album cover display: terminal album cover rendering via `ratatui-image`, auto-cropped to circle
- Multiple player bar layouts: `default`, `modern`, `minimal`, configurable via `playerbar.layout`
- Player bar component visibility config: `playerbar.visible` independently controls cover, volume, play mode, and loading animation
- Border gradient animation: `border_gradient` and `border_gradient_speed` options with clockwise flowing gradient effect
- Config example: new `config.example.toml` with complete documentation of all options
- Centralized API service layer (`service.rs`): unified endpoint resolution, cache integration, and error mapping
- Recursive local music scan: automatically scans audio files in subdirectories
- Search result limit: new `search_limit` config option
- Automatic cache eviction: LRU-based auto cleanup of cache over 2GB, with stale entry cleanup

### Changed

- Refactored all API calls from `self.api` to `self.service.client()`, decoupling business layer from API layer
- Player bar split into multi-module structure (`widgets`, `build_layout`, `default_layout`, `modern_layout`, `minimal_layout`)
- Cache index lock upgraded from `Mutex` to `RwLock` for better concurrent read performance
- NCM network retries reduced from 3 to 2 for faster fallback to YouTube source
- buffer underrun/overrun errors silently ignored, rodio auto-recovers
- `PlaybackEngine::new` now takes `CacheManager` directly instead of scattered path/template params
- Removed commented-out dev-dependencies in `Cargo.toml`

### Fixed

- Fixed cache index potentially containing incomplete download entries (now written only after download completes)
- Fixed stale entries from deleted files not cleaned in cache index (auto-cleaned on exit)
- Fixed local music scan missing audio files in subdirectories

## [0.1.2] - 2026-07-23

### Added

- Gradient progress bar (GradientLineGauge) with colorgrad preset themes
- Border config `BorderConfig` with `rounded` and `follow_corner_color` options
- Player progress bar gradient config: `gradient_enabled` and `gradient_preset`
- Cache index stores song duration to avoid decoding audio on playlist load
- Async cache methods: `load_lyrics_cache_async`, `list_cached_songs_async`
- YouTube search helper module (`utils/youtube.rs`) with traditional/simplified Chinese normalization and improved match scoring

### Changed

- Refactored event system: `AppEvent` split into five domain sub-events `SplashEvent`, `AuthEvent`, `PlaybackEvent`, `NavigationEvent`, `CommandEvent`
- Unified playback strategy into a single `Strategy` enum, removed `Box<dyn PlayStrategy>` dynamic dispatch
- Player `player::run` returns a oneshot completion signal, ensuring previous track's decoder/sink/StreamDownload fully released before next starts
- YouTube search helpers extracted from `AudioSource` into a separate module
- Removed example files in examples dir and dev-dependencies
- Added `rustfmt.toml` for unified code formatting

### Fixed

- Fixed resource leak from old player resources (HTTP connections, buffers) not released promptly on track switch
- Fixed cache index deserialization compat with old format (plain string → new object format smooth migration)

## [0.1.1] - 2026-07-21

### Added

- YouTube fallback playback via y7dl submodule
- User-created playlist API (`user_created_playlist`)
- User-collected playlist API (`user_collected_playlist`)
- `SongList` model adds `subscribed` field
- Navigation adds "My Created Playlists" and "My Collected Playlists" endpoints
- Cache manager supports indexed cache and custom filename templates
- New cache config options: `cache_dir`, `quality`, `cache_template`
- History queue limited to max 200 songs
- Heartbeat mode limited to max 500 songs with auto queue trimming
- Auto-select current song in content list during playback
- Liked Music auto-sets playlist ID to support Heartbeat mode

### Changed

- Refactored cache manager to use `cache_index.json` indexed cache
- Split "My Playlists" into "My Created Playlists" and "My Collected Playlists"
- Improved audio quality selection with configurable `SongQuality`
- Enhanced Heartbeat mode logging and error handling
- Fixed local file playback, improved local music scan using path-based unique IDs

### Fixed

- Fixed downloaded music showing 00:00 duration (reads actual duration from audio file)

### Documentation

- Updated license info to Apache-2.0 and added usage notes
- Added Windows Scoop installation instructions

## [0.1.0] - 2026-07-20

### Added

- Pigma initial release - terminal music player
- Playback engine supporting multiple audio formats (MP3, FLAC, WAV, OGG, AAC, M4A, WMA)
- NetEase Cloud Music API integration for streaming playback
- Local music scanning and playback
- Playlist management with auto save/restore
- Multiple play modes: sequential, single loop, list loop, random, heartbeat
- Volume control and progress seeking
- Lyrics display and translation support
- UI styled text rendering
- UI gradient theme support
- Downloaded/cached song management
- Search functionality
- Keyboard shortcut navigation
- Playback queue management
- Artist and album browsing
- Charts browsing
- QR code login

### Changed

- Refactored UI and utility modules for better performance and organization
- Refactored playback module and UI components
- Improved code readability and module organization
- CI workflow adds Linux audio dependency installation
- Refactored log initialization and enhanced playback features

### Fixed

- Simplified release targets for stable builds
- Fixed release workflow dependencies and artifact upload
- Ran cargo fmt to unify code style
