# pigma

[![CI](https://github.com/GBLMX/pigma/actions/workflows/ci.yml/badge.svg)](https://github.com/GBLMX/pigma/actions/workflows/ci.yml)
[![Release](https://github.com/GBLMX/pigma/actions/workflows/release.yml/badge.svg)](https://github.com/GBLMX/pigma/actions/workflows/release.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![AUR Version](https://img.shields.io/aur/version/pigma-gblmx-bin)](https://aur.archlinux.org/packages/pigma-gblmx-bin)
![GitHub repo size](https://img.shields.io/github/repo-size/GBLMX/pigma)


<img width="100" src="./imgs/logo.png" alt="pigma" />

pigma 的核心目标是把网易云音乐和本地音频播放的体验带进命令行环境：终端里的流式播放、歌词、歌单与队列管理，全部围绕键盘操作组织，基于 [Ratatui](https://ratatui.rs) 实现。

本仓库是 [akirco/pigma](https://github.com/akirco/pigma) 的 fork，**持续维护中**：上游的进展在这里跟进，本仓库自己也带了一批改动 —— 鼠标交互（点击 seek／切区／播放控制／模式／喜欢／静音）、频谱与音高读数、`:` 命令行与补全（含邮箱/短信登录）、鼠标可关(`mouse = false` 换回终端选中复制)、切歌/出错桌面通知(OSC 9)、歌词多种显示样式(`:lyrics window|one_line|flow|plain`)、听歌打卡、自定义主题（`[themes.<名>]` 继承内置主题，只写要改的颜色）、按终端能力自动适配配色与字形、按终端的图像与输入能力走原生协议(kitty 图形协议封面、同步刷新、kitty 键盘协议与括号粘贴；封面协议可用 `[playerbar] image_protocol` 强制)。二进制与 AUR 包都从**本仓库**发布（[releases](https://github.com/GBLMX/pigma/releases)、`pigma-gblmx-bin`）。

<details>
<summary><b>📖 点击展开/折叠目录 (Table of Contents)</b></summary>

- [pigma](#pigma)
  - [本仓库与原作者](#本仓库与原作者)
  - [Features](#features)
  - [Preview](#preview)
  - [Install](#install)
    - [From releases](#from-releases)
    - [From source (cargo)](#from-source-cargo)
    - [Build from source](#build-from-source)
  - [Usage](#usage)
    - [命令模式（vim 风格）](#命令模式vim-风格)
    - [CLI 控制（status / msg）](#cli-控制status--msg)
      - [直接走 Unix socket（socat / 脚本）](#直接走-unix-socketsocat--脚本)
      - [Windows：命名管道控制（PowerShell）](#windows命名管道控制powershell)
    - [无头守护进程模式（pigma -d）](#无头守护进程模式pigma--d)
  - [Configuration](#configuration)
    - [Columns Configuration](#columns-configuration)
      - [Column width types](#column-width-types)
      - [Available fields by content type](#available-fields-by-content-type)
      - [All override keys](#all-override-keys)
    - [Navigation layout](#navigation-layout)
    - [Title templates](#title-templates)
    - [Progress bar customization](#progress-bar-customization)
    - [Content cache](#content-cache)
    - [Splash screen](#splash-screen)
    - [Lyric gradient](#lyric-gradient)
    - [Navigation items](#navigation-items)
      - [Section titles support rich-text markup](#section-titles-support-rich-text-markup)
    - [Theme](#theme)
  - [Development](#development)
  - [Plan](#plan)
  - [License](#license)

</details>


### 本仓库与原作者

| | |
| --- | --- |
| **上游** | [akirco/pigma](https://github.com/akirco/pigma) —— 作者 akirco，Apache-2.0。原始版权与许可声明见 [LICENSE](./LICENSE)，**未作改动** |
| **本仓库** | GBLMX 的 fork，由 GBLMX 维护。这里的提交、[releases](https://github.com/GBLMX/pigma/releases) 与 AUR 包 `pigma-gblmx-bin` 都由本仓库负责，与原作者无关；上游是否采纳这些改动、上游自身的维护计划，本仓库不代表也不承诺 |
| **向上游贡献** | 上游的 [CONTRIBUTING](./CONTRIBUTING.md) 仍然适用。本仓库的 `main` 已与上游分叉，向上游提 PR 请从独立分支（如 `feat/...`）出发，不要从 `main` |

相对上游 `21c380d`（v0.2.14），本仓库自带的改动：

- **构建**：两个 crate 收进一个 Cargo workspace —— 依赖版本统一（rustls 三份规格合一）、`cargo test/clippy --workspace` 覆盖全部成员，CI 增加 ubuntu 与成员检查
- **修复**：下载缓存条目只在流完成后记录 · 默认日志级别改为 INFO · 清空的 `sections`/`columns` 序列化不再 panic · eapi 非 2xx 只告警 · IPC socket 权限收窄到属主 · `.gitignore` 忽略调试残留
- **配置**：`config_version` 版本号，旧文件加载时自动升级并把原文件备份为 `config.toml.bak-v0`
- **播放**：解析失败按类型分类（网络失败重试一次、无版权/无地址直接走兜底源），不再靠错误字符串前缀判断
- **外观**：符号预设（`nerd`／`unicode`／`ascii`，不装 Nerd Font 也能用）· 按终端能力降级真彩色 · 依据终端背景自动选明/暗主题
- **新增**：频谱可视化 · 音高读数（自实现 YIN，无新增依赖）· 鼠标交互（点击 seek／切区／播放控制／模式／喜欢／静音）· vim 风格 `:` 命令行与 Tab 补全（密码/短信登录、退出登录、签到）· 听歌打卡（播满约 30 秒即上报，与官方客户端口径一致；短于 30 秒的歌以播完为准）
- **打包**：AUR `pigma-gblmx-bin`（独立包名，发布时带真实校验和）

**注意：**

> 该项目仅供学习与研究使用。

**升级提示：`config.toml` 现在带 `config_version`，旧文件（没有该字段，按 v0 处理）在加载时会自动升级，并把原文件备份为 `config.toml.bak-v0`；反之，来自更新版本的配置文件按原样使用（未识别的字段忽略，不会被降级覆盖）。**

**[配置参考](./config.example.toml)**

**终端必须配置并使用支持 Nerd Fonts（如 JetBrainsMono Nerd Font, FiraCode Nerd Font 等）的字体，否则 `\uE0B2`等字符无法正确显示，会变成乱码或方块。**

## Features

- [x] 流式播放，边听边存
- [x] 低延迟seek
- [x] 本地音频播放
- [x] 自定义渲染导航列表
- [x] 自定义渲染内容列表
- [x] 歌词渐变逐字高亮
- [x] table标题自定义
- [x] 心动模式
- [x] 数据分页加载
- [x] kugou,kuwo,bilibili,youtube源fallback(无需cookie),参考[UnblockNeteaseMusic](https://github.com/UnblockNeteaseMusic/server)
- [x] 歌曲操作(like,dislike,fav .etc)
- [x] 重构播放队列
- [x] 下载管理（重合边听边存）
- [x] 重写playerbar(支持歌曲封面)
- [x] 云盘上传（缓存文件，本地文件）
- [x] 音量控制
- [x] 更多layout支持
- [x] 支持系统包管理器安装(yay,paru,scoop)
- [x] 支持搜索多源
- [x] 重构播放队列添加逻辑
- [x] 优化主题配色
- [x] styled_text标记语法嵌套
- [x] 重构进入程序流程
- [x] 命令行控制（status/msg）+ JSON IPC（waybar 等）
- [x] 守护进程模式（`pigma -d`）
- [x] 重写splash
- [ ] command panel重写，更多运行时配置支持
- [ ] 云盘源作为fallback
- [ ] 本地音频歌词，元数据重写
- [ ] landing page
- [ ] 歌手信息
- [ ] ~~修复手机验证码\邮箱登录~~
- [ ] ~~新增可选歌词页(沉浸式封面+歌词)~~
- [ ] ~~ascii art style 歌词~~

## Preview



<table>
  <tr>
    <td><img src="./imgs/image_001.png" width="100%" /></td>
    <td><img src="./imgs/image_002.png" width="100%" /></td>
  </tr>
  <tr>
    <td><img src="./imgs/image_003.png" width="100%" /></td>
    <td><img src="./imgs/image_005.png" width="100%" /></td>
  </tr>
</table>


## Install

> 本节所有命令都对应本仓库的 [releases](https://github.com/GBLMX/pigma/releases)；上游的安装渠道装到的是不含本仓库改动的版本。
>
> Note: the `gnu` Linux builds depend on system audio libraries (e.g. `alsa-lib`).

### From releases



```sh
# https://github.com/marcosnils/bin
bin install https://github.com/GBLMX/pigma
```

`windows`

从 [releases](https://github.com/GBLMX/pigma/releases) 下载 `pigma-x86_64-pc-windows-msvc.zip`（或 `aarch64` 版），解包后把 `pigma.exe` 放进 `%PATH%`。

`linux(aur)`
```sh
yay -S pigma-gblmx-bin

#or

paru -S pigma-gblmx-bin
```

`macOS`

从 [releases](https://github.com/GBLMX/pigma/releases) 下载 `pigma-x86_64-apple-darwin.tar.gz`（Apple Silicon 用 `aarch64` 版），解包后把 `pigma` 放进 `$PATH`。

### From source (cargo)

```sh
cargo install --git https://github.com/GBLMX/pigma.git
```

### Build from source

`crates/sonar` 用到 `crates/y7dl` 子模块，克隆时要一并取回：

```sh
git clone --recurse-submodules https://github.com/GBLMX/pigma.git
cd pigma
cargo build --release
# binary at target/release/pigma
```

## Usage


| 快捷键        |                     描述                     |
| :------------ | :------------------------------------------: |
| w             |                 清空播放队列                 |
| s/d           |       添加到喜欢/不感兴趣(仅每日推荐)        |
| ?             |                  快捷键面板                  |
| r             |               手动刷新列表内容               |
| tab/shift+tab |              切换导航/搜索引擎               |
| enter         |                播放/进入列表                 |
| space         |                     暂停                     |
| f             |                   播放队列                   |
| l             |                     歌词                     |
| /             |                  搜索/过滤                   |
| b             |                   样式切换                   |
| left /right   |                   seek 15s                   |
| p /n          |                上一首/下一首                 |
| ctrl+p        |                command panel                 |
| L             |               登录网易云                     |
| c             |  切换表格为cell/row模式(回车进入歌手/专辑)   |
| m             | 切换播放模式（适用于我的歌单或我喜欢的音乐） |
| u             |  上传`本地音乐`或`下载管理`的音频到音乐云盘  |
| g/G           |                列表顶部/底部                 |
| v             |        频谱显示开关（同 `:visualizer on`）     |
| V             |          音高读数开关（同 `:pitch on`）        |
| :             |      命令模式（vim 风格，Tab 补全，见下节）     |

### 命令模式（vim 风格）

按 `:` 打开命令行：`Enter` 执行、`Esc` 取消、`Tab` 补全（命令名、`:theme` 的主题名、`on`/`off`）。
结果与错误都以 toast 回报，跟 vim 一样（例如 `E: 未知命令: foo`）。

| 命令 | 说明 |
|---|---|
| `:q` / `:quit` | 退出 |
| `:help` / `:login` | 快捷键面板 / 登录页 |
| `:save` | 立即写回 `config.toml` |
| `:theme <名字>` | 切换主题（`Tab` 会列出全部主题名） |
| `:volume 75` / `:volume +5` / `:volume -10` | 音量（与 `pigma msg volume` 同一套语法） |
| `:seek 90` / `:seek +15` / `:seek -30` / `:seek 50%` | 跳到某秒 / 相对跳转 / 百分比 |
| `:visualizer on\|off` | 频谱显示开关（同 `v` 键） |
| `:signin <账号> <密码>` | 邮箱或手机号 + 密码登录（`:login` 仍是二维码页） |
| `:sms <手机号>` | 发送短信验证码 |
| `:smslogin <手机号> <验证码>` | 短信验证码登录 |
| `:logout` | 退出登录（清服务端会话与本地 cookie） |
| `:sign` | 网易云每日签到（云贝） |
| `:layout default\|modern\|minimal` | 播放条布局（`modern` 下频谱只有封面列的 8 格宽，另两种布局是整行） |
| `:pitch on\|off` | 音高读数开关（同 `V` 键） |

终端背景为浅色时，`:theme` 配合配置里的 `background = "auto"` 与 `light_theme` 会自动选浅色主题。

### CLI 控制（status / msg）

查询/控制一个**正在运行**的 pigma 实例（交互界面或守护进程均可），
通过 `~/.cache/pigma/pigma.sock` 上的 Unix socket 通信：

```bash
# 生成 shell 补全脚本（bash / zsh / fish / elvish / powershell）
# bash
pigma completions bash > ~/.local/share/bash-completion/completions/pigma

# zsh
pigma completions zsh > "${fpath[1]}/_pigma"

# fish
pigma completions fish > ~/.config/fish/completions/pigma.fish

# powershell：生成脚本并在 $PROFILE 中自动加载（在 pwsh 里执行）
pigma completions powershell | Out-File "$HOME/.config/powershell/pigma.ps1" -Encoding utf8
Add-Content $PROFILE '. "$HOME/.config/powershell/pigma.ps1"'

```

| 命令 | 说明 |
|---|---|
| `pigma status` | 查询状态（默认 plain 文本） |
| `pigma status --json` | 以 JSON 输出 |
| `pigma status -L` | 列出当前播放队列（`>` 标记当前曲目），`-L --json` 输出原始 `QueueSnapshot` |
| `pigma status --template "{name}  {artist}  {current}/{duration}  {status}  vol {volume}%"` | 自定义 plain 输出模板 |
| `pigma msg list` | 列出当前播放队列（`▶` 标记当前曲目），`--json` 输出原始 `QueueSnapshot` |
| `pigma msg next` / `pigma msg previous` | 下一首 / 上一首 |
| `pigma msg pause` / `pigma msg play` | 暂停 / 播放（`pigma msg play <song-id>` 按 id 跳播队列中的歌曲） |
| `pigma msg search <keyword>` | 搜索并返回歌曲数据（NCM + 已启用 sonar 源，标出 `source` 和 `id`），再 `pigma msg play <id>` 播放选中的那首 |
| `pigma msg toggle_play` | 播放/暂停切换 |
| `pigma msg mode` | 切换播放模式 |
| `pigma msg like` / `pigma msg dislike` | 喜欢 / 不喜欢 |
| `pigma msg toggle_like` | 喜欢/取消喜欢（切换当前曲目） |
| `pigma msg switch-list <endpoint>` | 动态切换守护进程的队列到指定端点（如 `recommend_songs`、`toplist`），歌单端点可用 `--playlist N` 选第 N 个 |
| `pigma msg volume 75` | 绝对音量（0-100） |
| `pigma msg volume +5` / `-10` | 相对 ±%（与 TUI 的 `+` / `-` 一致，支持负数） |

`pigma status` 的 `--template` 支持占位符：`{name}` `{artist}` `{album}` `{current}`/`{position}`
`{duration}` `{volume}` `{status}` `{mode}` `{id}` `{liked}`。
未指定时默认模板来自配置项 `cli_status_template`；`--json` 优先于配置项
`cli_status_format`（见 [config.example.toml](./config.example.toml)）。

#### 直接走 Unix socket（socat / 脚本）


> **仅 Linux/macOS**：`status` / `msg` 子命令底层就是往 `~/.cache/pigma/pigma.sock`
> 发一行 JSON。Windows 用的是命名管道，见下节。
> 不想用 `pigma` 二进制时，可用 `socat` 或任何 Unix socket 客户端直接控制：

```bash
# 查询状态（返回一行 JSON）
printf '{"cmd":"status"}\n' | socat - "$HOME/.cache/pigma/pigma.sock"

# 列出播放队列
printf '{"cmd":"list"}\n' | socat - "$HOME/.cache/pigma/pigma.sock"

# 播放控制（注意 action 是嵌套对象，与 pigma msg 实际发送的 JSON 一致）
printf '{"cmd":"msg","action":{"action":"next"}}\n'          | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"previous"}}\n'      | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"pause"}}\n'         | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"play"}}\n'          | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"mode"}}\n'          | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"like"}}\n'          | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"dislike"}}\n'       | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"toggle_like"}}\n'   | socat - "$HOME/.cache/pigma/pigma.sock"

# 音量：绝对（0.0-1.0）或相对增量
printf '{"cmd":"msg","action":{"action":"volume","absolute":0.75}}\n' | socat - "$HOME/.cache/pigma/pigma.sock"
printf '{"cmd":"msg","action":{"action":"volume","delta":0.05}}\n'    | socat - "$HOME/.cache/pigma/pigma.sock"

# 切换队列到指定端点（歌单端点可用 "playlist" 选第 N 个，1 起始）
printf '{"cmd":"msg","action":{"action":"switch_list","endpoint":"toplist","playlist":2}}\n' | socat - "$HOME/.cache/pigma/pigma.sock"
```

约定：每行请求须以换行结尾，服务端每连接处理一个请求并回一行 JSON
（`msg` 成功回 `{"ok":true}`）。socket 路径可用 `--socket <path>` 自定义。

#### Windows：命名管道控制（PowerShell）

Windows 上 IPC 走命名管道 `\\.\pipe\pigma`（可用 `--socket <pipe-name>` 自定义），
协议相同（一行 JSON + 换行，服务端回一行 JSON）。用 PowerShell 控制：

```powershell
function Send-Pigma($json) {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'pigma', [System.IO.Pipes.PipeDirection]::InOut)
    $pipe.Connect(5000)
    $sw = New-Object System.IO.StreamWriter($pipe)
    $sw.NewLine = "`n"
    $sw.WriteLine($json); $sw.Flush()
    $sr = New-Object System.IO.StreamReader($pipe)
    return $sr.ReadLine()
}

Send-Pigma '{"cmd":"status"}'                 # 查询状态
Send-Pigma '{"cmd":"list"}'                   # 列出播放队列
Send-Pigma '{"cmd":"msg","action":{"action":"next"}}'    # 下一首
Send-Pigma '{"cmd":"msg","action":{"action":"volume","absolute":0.75}}'  # 音量 75%
```

### 无头守护进程模式（pigma -d）

以无终端方式后台运行（可挂在 waybar / systemd 下），加载指定的 API 作为初始队列（**不自动播放**，用 `pigma msg play` 或 waybar 的 toggle 按钮开始）。

| 选项 | 说明 |
|---|---|
| `pigma -d` | 等价于 `pigma -d liked` |
| `pigma -d toplist` | 加载指定端点 |
| `pigma --daemon user_cloud_disk` | `-d` 的全写形式 |
| `pigma -d toplist:3` | 歌单/榜单端点用 `:N` 选第 N 个（1 起始） |

支持的内置 API 与导航项一致：

| 端点 | 类型 | 说明 |
|---|---|---|
| `liked` | 歌曲（默认） | 我喜欢的音乐，需登录 |
| `recommend_songs` | 歌曲 | 每日推荐歌曲 |
| `user_cloud_disk` | 歌曲 | 我的云盘 |
| `download` | 歌曲 | 本地下载 |
| `local_music` | 歌曲 | 本地音乐 |
| `recent` | 歌曲 | 最近播放 |
| `recommend_resource` | 歌单 | 每日推荐歌单 |
| `toplist` | 歌单 | 排行榜 |
| `top_song_list` | 歌单 | 热门歌单 |
| `user_radio_sublist` | 歌单 | 我的电台 |
| `user_song_list` | 歌单 | 用户歌单 |
| `user_created_song_list` | 歌单 | 创建的歌单 |
| `user_subscribed_song_list` | 歌单 | 订阅的歌单 |
| `album_sublist` | 歌单 | 收藏的专辑 |
| `search` | 其他 | 搜索热榜（无可播队列） |
| `top_singers` | 其他 | 热门歌手（无可播队列） |

歌单类端点解析出来是一组歌单，默认加载第一个；`-d` 可用 `ENDPOINT:N`（如 `pigma -d toplist:3`）选择第 N 个（1 起始），`msg switch-list` 可用 `--playlist N`。序号与 TUI 中列表显示的顺序一致，可先在 TUI 里查看：

启动后即用 `pigma status` / `pigma msg` 控制；`SIGINT`/`SIGTERM` 会保存会话并干净退出。

**Waybar 集成**：

  - 参考[waybar](./waybar)。状态模块用 bash 脚本 `waybar/pigma`，每秒调 `pigma status --json` 获取状态并格式化为 waybar JSON。脚本内置 `ensure_daemon`，首次调用时自动启动 daemon。

```jsonc
// ~/.config/waybar/config.jsonc 核心片段
"custom/pigma": {
  "exec": "~/.config/waybar/scripts/pigma",
  "interval": 1,
  "return-type": "json",
  "on-click-right": "pigma msg mode",
  "on-scroll-up": "pigma msg volume +5",
  "on-scroll-down": "pigma msg volume -5"
}
```

## Configuration
Config file location:

- linux: `~/.config/pigma/config.toml`
- macOS: `$HOME/Library/Application Support/pigma/config.tomnl`
- windows:`RoamingAppData`

### 配置版本与迁移

`config.toml` 顶部写着 `config_version`（当前为 `1`）：

```toml
config_version = 1
```

- 缺少该字段的文件按 **v0**（引入版本号之前写下的）处理，加载时自动升级，并把原文件备份为
  `config.toml.bak-v0`，升级结果在下次 `save()` 时写回；
- 已经是当前版本的文件不会被迁移，也不会产生备份；
- 来自更新版本的文件按原样使用，未识别的字段忽略（不会被旧版本覆盖降级）；
- 字段改名/删除/语义变化时递增 `CONFIG_VERSION`，并在 `Config::migrate_from` 里加一步迁移。

### Columns Configuration

Each content type has two levels of columns: **type-defaults** and **per-API overrides**.


```toml
[columns]
songs = [
    { header = "TITLE", field = "name", min_width = 18 },
    { header = "ARTIST", field = "singer", width = 16 },
    { header = "ALBUM", field = "album", min_width = 12 },
    { header = "DURATION", field = "duration", width = 9 },
]
songlist = [
    { header = "NAME", field = "name", min_width = 20 },
    { header = "AUTHOR", field = "author", width = 16 },
]

[columns.overrides]
toplist = [
    { header = "NAME", field = "name", width = 20 },
    { header = "DESCRIPTION", field = "description", min_width = 20 },
]
search = [
    { header = "HOT SEARCH", field = "keyword", min_width = 1 },
]
```


#### Column width types

| Format           | Description               |
| ---------------- | ------------------------- |
| `width = 16`     | Fixed width in characters |
| `min_width = 18` | Minimum width, flex grows |
| `ratio = [1, 3]` | Proportional ratio weight |

#### Available fields by content type

**`songs`** (SongInfo) — used by these APIs:

| API               | Description         |
| ----------------- | ------------------- |
| `recommend_songs` | 每日推荐            |
| `user_cloud_disk` | 我的音乐云盘        |
| `recent_songs`    | 最近播放            |
| `liked_songs`     | 我喜欢的音乐        |
| `local_music`     | 本地音乐            |
| Playlist entry    | 歌单/排行榜内的歌曲 |

Fields:

| field      | Type   | Notes                            |
| ---------- | ------ | -------------------------------- |
| `name`     | String | 歌曲名                           |
| `singer`   | String | 歌手                             |
| `album`    | String | 专辑                             |
| `duration` | String | 时长，已格式化为 `MM:SS`（自动） |

**`songlist`** (SongList) — used by these APIs:

| API                  | Description |
| -------------------- | ----------- |
| `recommend_resource` | 推荐歌单    |
| `top_song_list`      | 歌单        |
| `user_radio_sublist` | 电台        |
| `user_song_list`     | 我的歌单    |

Fields:

| field    | Type   | Notes  |
| -------- | ------ | ------ |
| `name`   | String | 歌单名 |
| `author` | String | 作者   |

**`toplist` (override)** (TopList):

| API       | Description |
| --------- | ----------- |
| `toplist` | 排行榜      |

Fields:

| field         | Type   | Notes  |
| ------------- | ------ | ------ |
| `name`        | String | 榜单名 |
| `description` | String | 描述   |

**`singers`** (SingerInfo) — used by these APIs:

| API           | Description |
| ------------- | ----------- |
| `top_singers` | 热门歌手    |

Fields:

| field  | Type   | Notes   |
| ------ | ------ | ------- |
| `name` | String | 歌手名  |
| `id`   | u64    | 歌手 ID |

**`search` (override)** (HotSearch):

| API      | Description |
| -------- | ----------- |
| `search` | 搜索-热搜榜 |

Fields:

| field     | Type   | Notes      |
| --------- | ------ | ---------- |
| `keyword` | String | 搜索关键词 |

#### All override keys

Any API endpoint can have a `[columns.overrides.{key}]` entry. Available keys:

| Key                  | Default type | Description  |
| -------------------- | ------------ | ------------ |
| `recommend_songs`    | songs        | 每日推荐     |
| `recommend_resource` | songlist     | 推荐歌单     |
| `toplist`            | toplist      | 排行榜       |
| `top_song_list`      | songlist     | 歌单         |
| `user_radio_sublist` | songlist     | 电台         |
| `user_cloud_disk`    | songs        | 我的音乐云盘 |
| `liked`              | songs        | 我喜欢的音乐 |
| `user_song_list`     | songlist     | 我的歌单     |
| `local_music`        | songs        | 本地音乐     |
| `recent`             | songs        | 最近播放     |
| `top_singers`        | singers      | 热门歌手     |
| `search`             | songs        | 搜索-热搜榜  |
| `download`           | —            | 下载管理     |

### Navigation layout

```toml
# 导航栏位置: "left" (左侧边, 默认) 或 "top" “right” "bottom"
navigation_position = "left"
```

`top` 模式下导航项横排为一行，超宽时自动横向滚动，Tab/BackTab 切换导航项不变。

### Title templates

```toml
[titles]
sidebar = "NAVIGATION"
playlist = "\u266a QUEUE ({count})"  # {count} = song count
lyrics = "\u266a LYRICS"
```

`{name}` and `{count}` placeholders are supported in the NavItem title template.

### Progress bar customization

```toml
[playerbar]
# 播放栏布局: "default", "modern", "minimal"
layout = "modern"

# 进度条填充符号
filled_symbol = "━"
# 进度条未填充符号
unfilled_symbol = "─"
# 进度条填充颜色 (颜色名或 hex)
filled_color = "accent"
# 进度条未填充颜色
unfilled_color = "text"
# 已缓存到本地时进度条轨道颜色
unfilled_color_cached = "warning"
# 是否启用进度条渐变效果
gradient_enabled = false
# 渐变预设: "warm", "cool", "sunset", "ocean", "forest", "neon", "pastel", "rainbow"
gradient_preset = "warm"

# 播放栏各组件可见性(建议暂时别用，音量控制没写好)
[playerbar.visible]
# 是否显示封面
cover = true
# 是否显示音量控制
volume = true
# 是否显示播放模式图标
mode_icon = true
# 是否显示加载动画
spinner = true
```

Supported theme color names: `bg`, `surface`, `text`, `accent`, `highlight`, `muted`, `error`, `warning`.

### Content cache

```toml
content_cache_ttl = 300  # seconds, 0 to disable
```

### Splash screen

启动 splash 界面的进度条按设定时长播放动画，时间到了自动跳转到对应界面
（未登录→主界面公开内容，已登录→主界面，离线→本地音乐）：

```toml
splash_duration_secs = 2.0
```

### Lyric gradient

歌词当前行高亮渐变风格（自实现，无额外依赖）：

```toml
lyric_gradient = "warm"  # warm | cubehelix | rainbow | spectral | viridis | turbo
```

未知值回退到 `warm`。

### Navigation items

Each nav item can have:

```toml
[[navigation.sections.items]]
name = "推荐歌单"
api = "recommend_resource"
title_template = "{name} ({count})"
```

#### Section titles support rich-text markup

The `title` of a `[[navigation.sections]]` entry supports inline markup tags that
are styled by the active theme:

| Tag                  | Meaning      |
| -------------------- | ------------ |
| `<accent>…</accent>` | Accent color |
| `<b>…</b>`           | Bold         |


**支持的标记语法**

> 标记语法不限于导航列表，表格 block title、表格标题等均支持。

| 类型 | 标签 | 含义 | 写法示例 |
| --- | --- | --- | --- |
| 主题色 | `<accent>` | 主题强调色 | `<accent>►</accent>` |
| 主题色 | `<text>` | 主题正文色 | `<text>…</text>` |
| 主题色 | `<muted>` | 主题弱化色 | `<muted>…</muted>` |
| 主题色 | `<error>` | 主题错误色 | `<error>…</error>` |
| 主题色 | `<bg>` | 主题背景色 | `<bg>…</bg>` |
| 主题色 | `<surface>` | 主题面板色 | `<surface>…</surface>` |
| 主题色 | `<border>` | 主题边框色 | `<border>…</border>` |
| 修饰符 | `<b>` | 加粗 | `<b>DISCOVER</b>` |
| 修饰符 | `<i>` | 斜体 | `<i>…</i>` |
| 修饰符 | `<dim>` | 弱化 | `<dim>…</dim>` |
| 字面颜色 | `<#rrggbb>` | 十六进制颜色 | `<#ff5500>…</#ff5500>` |
| 字面颜色 | `<任意颜色名>` | ratatui 支持的颜色名（如 `red`、`blue`） | `<red>…</red>` |
| 渐变色 | `<gradient:preset>…</gradient>` | 逐字符渐变 | `<gradient:rainbow>…</gradient>` |
| 渐变色 | `<grad:preset>…</grad>` | 逐字符渐变（简写） | `<grad:turbo>…</grad>` |

渐变预设：`warm`、`cubehelix`、`rainbow`、`turbo`、`spectral`、`viridis`。

- 普通标签（主题色/修饰符/字面颜色）支持嵌套，例如 `<b><accent>DISCOVER</accent></b>` 会得到加粗的强调色文本。
- 渐变标签内的内容不再解析内部标签，整段按字符上渐变色；渐变标签需成对使用（`</gradient>` 或 `</grad>`）。
- 未含标签的文本按无样式渲染。

**支持标记语法的内容**

| 内容 | 配置项 | 位置 |
| --- | --- | --- |
| 表格列标题 | `[columns]` 或 `[columns.overrides]` 中的 `header` | content 表格 |
| 导航区块标题 | `[[navigation.sections]]` 的 `title` | 侧边导航 |
| 导航项名称 | `[[navigation.sections.items]]` 的 `name` | 侧边导航 |
| 面包屑 | 沿用导航配置的 section `title` / item `name`，无独立配置项 | 顶部面包屑 |
| Block 标题 | `title_template`（含 `{name}` `{count}` `{total}` 占位符，先替换再解析标记） | 内容区、队列、歌词页、帮助、命令面板等 |

> 表格**行内容**（歌曲名、歌手等单元格）暂不支持标记语法，按纯文本渲染。


Example (the default):

```toml
[[navigation.sections]]
title = "<accent>▎</accent> <b>DISCOVER</b>"

[[navigation.sections.items]]
name = "每日推荐"  # 同样支持title的标记语法
api = "recommend_songs"
title_template = "{name} ({count})" # 同样支持title的标记语法
```

### Theme

pigma no longer ships built-in themes. You must define one or more `[[themes]]`
entries in your config, and select the active one via `default_theme` (matched by
`name`). If `themes` is empty, the UI falls back to a built-in default palette.

```toml
default_theme = "rose-pine"

[[themes]]
name = "rose-pine"
bg = "#191724"
surface = "#26233A"
text = "#E0DEF4"
accent = "#EB6F92"
highlight = "#31748F"
muted = "#6E6A86"
error = "#EB6F92"
warning = "#F6C177"
```

Supported theme color fields: `bg`, `surface`, `text`, `accent`, `highlight`,
`muted`, `error`, `warning`.

You can define multiple themes and switch between them at runtime (style toggle,
default key `b`).

## Development

```sh
git clone https://github.com/GBLMX/pigma.git
cd pigma
git submodule update --init --recursive
cargo run
cargo +nightly fmt
```

## Plan

- 完善 waybar/systemd 集成文档与示例配置
- 守护进程模式下更多端点的支持（榜单/歌单自动展开）
- `pigma msg` 更多动作（seek、queue 操作等）

## License

Licensed under the [Apach-2.0](LICENSE) license.
