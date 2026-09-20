# 框架与结构调整

这份文档记录 boxpigma 里"把自写实现换成成熟方案"的决策、依据与代价，以及**同样重要**的
**被否决的候选**。判据只有一条：

> **新增依赖必须能对应到一个已量到的问题；量不到就不加。**

反面例子（本仓库已经拒绝过的）见下面「已否决」一节。判断"哪些必须继续自己写"时用同一条尺子：
成熟库确实不提供，或换过去会改变契约 —— 那就保留自写。

---

## 一、已采纳

| 方面 | 改前 | 改后 | 依据（实测） | 位置 |
| --- | --- | --- | --- | --- |
| 日志 | 手写 `log::Log` 实现：`Mutex<File>` 追加写，**无轮转、无上限**，调用线程持锁同步写 | `tracing` + `tracing-subscriber` + `tracing-appender`：按天轮转、保留最近 7 个、行内带模块路径与本地时间 | 旧日志实测已达 **2,892,695 B** 且无上限；替换后**135 个 `log::*!` 调用点一行未改**（`tracing-log` 桥接 `log` 记录） | `src/logger.rs` |
| 对比度 | 手写 WCAG 亮度/对比度（`relative_luminance` / `contrast_ratio`），且 `ui.rs` 的审计测试里还有**第三份**拷贝 | `palette` 的 `Wcag21RelativeContrast`；全仓颜色数学收敛到一处 | 全 **2²⁴ 个 8 位 sRGB 颜色**逐值对拍，最大亮度偏差 7.29e-5（palette 用 CIE/Lindbloom 全精度系数，旧代码用 WCAG 取整系数）；**19 个内置主题的 `on_accent` 结果全部一致**；唯一分歧带是黑白锚对比度打平处（合成扫描的 0.005%，可读性相同，无内置主题落在那） | `src/config/theme.rs`、`src/app/theme.rs`、`src/ui.rs` |
| 桌面通知 | 只用 `OSC 9`（单串正文） | kitty 走其自有的 `OSC 99`：标题与正文分开（`p=`）、Base64 负载（`e=1`）、`f=` 声明应用名便于过滤、`i=`/`d=` 分块并避免堆叠 | kitty 官方文档：kitty 实现 `OSC 99`，同时**兼容**旧式 `OSC 9`；其余终端保持 `OSC 9` **逐字节不变**（有测试钉住） | `src/utils/terminal.rs` |
| 网络超时 | 只有 `ncm-api` 设了 30s；sonar 搜索、封面下载、音频流**一处超时都没有** | 统一 `connect_timeout` 10s + `read_timeout` 30s；短请求另加 30s 总时限 | **音频流刻意不加总超时**（会截断正在播放的下载）—— 用 34 秒持续下载的探针实证不会被切断；`read_timeout` 是每次读的超时、读完即重置，语义已在 reqwest 源码里核实 | `src/app/builder.rs`、`crates/sonar/src/provider.rs` |
| 页面结构 | 绘制分发、页面按键、键位表三处各自维护 | 一张表：`PageSpec { name, key, render }` + `Page::spec()/opened_by()/on_key()` | 新增一个页面从改 **9 个文件降到 2 个**；`ui.rs` 生产代码里的页面匹配臂 5 → 0、`layout.rs` 3 → 0；等价性用旧/新代码各渲染 14 个场景比对缓冲区哈希验证 | `src/state/page.rs`、`src/ui.rs`、`src/layout.rs` |
| 铺底色 | `Block::default().style(bg)`（用一个无边界的 `Block` 只为刷背景） | `Fill::new(" ")` | `Fill` 的渲染就是 `set_symbol + set_style`，像素等价；换完之后编译器指出 `Block` 在该文件已无其它用途 | `src/ui.rs` |
| 启动画面字形 | 手绘 3 行 ASCII 字（B 的下面两行相同、X 的交叉挤在一行里，都读不清） | FIGlet 字体 **`Calvin S`** 的渲染结果，3 行 × 23 列，作为常量内嵌 | 用自写的 `.flf` 解析器（含 kerning 布局）比较 13 款字体后选定；运行时**不带字体依赖** | `src/ui/splash.rs` |
| 主题 | 主题名取自 `HashMap`，`:theme` 循环顺序每次启动都不同 | 名字排序固定；新增 `random`（启动时落定一次、`:theme random` 重掷并写回） | 排序后 `:theme` 顺序稳定；`random` 只在**设置主题时**解析，绝不在每帧的 `resolve_theme` 里掷（否则每帧换色） | `src/config/theme.rs` |

**结构性调整**（不属于依赖层面，但同样是"把决定只写一次"）：

- **启动画面的高度由字形决定**：此前布局写死 3 行、渲染按 0..2 手工索引 —— 加到第 4 行就 panic。
  现在 `layout::splash` 接收行数、渲染从 `LOGO.len()` 推导，字形是唯一写下来的地方。
- **日志调用点零改动**：`tracing-log` 桥接意味着换栈不必改 135 处调用点 —— 这是选它的首要理由。
- **主题解析只在"设置主题"时发生**：`resolve_theme` 每帧都跑，任何随机性都不能放在那里。

## 二、已否决（附依据）

| 候选 | 否决理由（实测） |
| --- | --- |
| `unicode-truncate` 替换手写 CJK 截断 | **不等价**：124 万组模糊测试显示分歧全部落在 emoji/ZWJ/控制字符上（按**字素簇**边界 vs 按字符；控制字符宽度算 1 vs 算 0）。测试不能被迁就，因此保留自写，依赖已移除 ✗ |
| `nucleo` 替换搜索匹配器 | 主搜索是**远端**完成的，本地只有 20 行子串过滤，UI 也**没有匹配字符高亮** —— 换上去是**功能变更**而不是优化 |
| `palette` 替换 16/256 色量化 | `palette` **不提供**终端调色板量化（已通读其源码确认），这部分手写必须保留 |
| `colorgrad` 替换渐变预设 | 只在全局 registry 缓存里、非项目依赖；6 个预设中 5 个与自写实现逐值一致、Cubehelix 不同 —— 收益不足以引入一棵依赖树 |
| tokio `taskdump` 做进程内卡死诊断 | 接线正确（信号确实到达、快照确实被采集到），但把快照**渲染**成回溯那一步在本机不返回，且采集时的 `poll` 会卡住工作线程、外层 `timeout` 来不及生效 —— 一次信号就能让监听永久失效。改用外部抓栈（见 README 的「排障」） |
| `tracing` 的 `EnvFilter` | 需要 `regex-automata` 等一串依赖；配置只给一个全局级别，用内置的 `LevelFilter` 即可 |
| `reqwest-retry` / `backon` | 未在本机 vendor，未核实；现有手写重试只有 8 处且各有明确语义（NCM 重试一次、bilivideo 退避重签等），先量化问题再谈 |
| `insta` 快照测试 / `criterion` 基准 | `criterion` 被项目自己的注释拒绝过（"Criterion would be a dependency tree for a handful of numbers"，见 `lib.rs` 的 `bench_util`），本仓库自带的 `#[ignore]` 基准已够；`insta` **尚未采纳** —— 把 60 余条 `TestBackend` 渲染测试快照化是候选，但目前没有量到的问题驱动它 |

**继续自己写的（成熟库确实不提供 / 换了会改契约）**：终端协议兜底判定（kitty/ghostty/sixel/tmux）、
封面圆形 alpha 遮罩、OSC 11 背景亮度、16/256 色量化、本地 DSP（FFT/YIN）、卡拉 OK 逐字符着色、
NCM 协议加密与签名、`:` 小语法解析、缓存淘汰与索引策略、配置版本迁移。

## 三、代价与收益

| 指标 | 数值 |
| --- | --- |
| 新增 crate | **7 个**（`tracing-appender`、`tracing-log` + `crossbeam-channel`、`crossbeam-utils`、`phf`、`phf_shared`、`symlink`），lock 增加 80 行 |
| 本来就在 lock 里、只是改为直接依赖 | `tracing`、`tracing-subscriber`（经 color-eyre 的 `tracing-error`）、`palette`、`base64` |
| 内存占用 | TUI **14.6 MB** / 守护进程 **13.7 MB**（旧版 14.0 / 13.4 MB）—— 增量 **+0.3~0.6 MB**，与日志栈换新同量级，未逐项归因；四种情况的启动峰值 = 稳态，24 秒内不增长 |
| 二进制体积 | 10,800,688 B → **11,145,688 B**（**+345,000 B ≈ +337 KiB / +3.2%**），含整条日志栈 |
| 性能 | **无回退**：分析帧 129.57 µs（30 fps 占单核 0.389%）、playerbar 整帧 79.15 µs（0.237%）、fft 2048 点 21.76 µs、封面 488–583 µs/首 —— 可由 `cargo test --release --lib -- --ignored --nocapture` 复现 |
| 自写代码 | 净减少：日志实现整体删除、颜色数学三处合并为一处、`Block` 铺底与手绘字形被替换 |

## 四、同步上游时的含义

结构性调整决定了哪些文件是"我们的地盘"：同步一律 **cherry-pick、不整体合并**，冲突时保留本仓库
版本并确认行为没有退化。逐文件清单与验收命令见 [CONTRIBUTING](./CONTRIBUTING.md) 的
「与上游同步」一节；与上游的差异也按"结构性 / 增量"两分类列在 [README](./README.md)。
