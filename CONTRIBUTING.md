# 贡献指南

感谢你考虑为这个项目做出贡献！我们欢迎任何形式的贡献，无论是修复 bug、改进文档，还是添加新功能。这份指南将帮助你快速上手。

---

## 行为准则

本项目遵循 [贡献者公约](https://www.contributor-covenant.org/version/2/1/code_of_conduct/)。参与即表示您同意遵守其条款。

---

## 如何贡献

除了提交代码，你还可以通过以下方式帮助项目：

- 报告 Bug 或功能需求（提交 Issue）
- 改进交互逻辑或UI建议
- 参与 Issues 讨论，帮助其他用户
- 在社交媒体上宣传项目

---

## 报告问题

### 提交 Issue

在提交 Issue 前，请先搜索现有 Issues，避免重复。

- **Bug 报告**：请使用 `Bug report` 模板，并详细描述复现步骤、预期行为与实际行为、环境信息（OS、版本等）。
- **功能请求**：请使用 `Feature request` 模板，清晰说明使用场景和预期收益。
- **其他需求**：请使用 `Other`模板，清晰描述问题。


---

## 开发环境搭建

1. **克隆仓库**：
   ```bash
   git clone --recurse-submodules https://github.com/GBLMX/pigma.git
   cd pigma
   ```

2. **确保 Rust 已安装**：
   ```bash
   rustup toolchain install stable
   ```

3. **安装依赖**：
   ```bash
   cargo build
   ```

4. **运行**：
   ```bash
   cargo run
   ```
5. **可能用到的工具推荐**
   ```
   cargo install git-cliff
   cargo install cargo-bloat
   cargo install cargo-whatfeatures
   cargo install cargo-flamegraph
   ```

---

## 贡献工作流

### 1. Fork & 分支

- **Fork 项目**到你的 GitHub 账户。
- 从 `main` 分支创建一个**新功能分支**，命名遵循 `feat/描述`、`fix/描述` 或 `docs/描述` 等格式。
  ```bash
  git checkout -b feat/你的功能名
  ```
- **注**：特别的功能实现或存在不确定可先提issue沟通实现思路

### 2. 编码与提交规范

- **代码风格**：运行 `cargo fmt` 自动格式化；运行 `cargo clippy` 检查常见错误。
- **提交信息格式**：必须遵循 [约定式提交](https://www.conventionalcommits.org/) 规范。这有助于自动生成变更日志（我们使用 `git-cliff`）。
  - 格式：`<类型>: <简短描述>`，如 `feat: 添加用户登录接口`、`fix: 修复内存泄漏问题`。
  - 允许的类型：`feat`、`fix`、`docs`、`style`、`refactor`、`perf`、`test`、`chore`。
  - 如有破坏性变更，在脚注中添加 `BREAKING CHANGE: 描述`。

### 3. 测试

- 新增功能或修复 Bug 时，请添加相应的测试用例。
- 运行 `cargo test` 确保所有测试通过。

### 4. 推送并创建 Pull Request (PR)

- 推送分支到你的 Fork：
  ```bash
  git push origin feat/你的功能名
  ```
- 前往原始仓库，点击 **New Pull Request**。
- **目标分支**：选择 `main`（受保护，需通过 PR 合并）。
- **PR 标题**：也建议遵循约定式提交（如 `feat: 添加...`），便于记录。
- **PR 描述**：请填写以下内容：
  - **变更说明**：清晰描述你做了什么。
  - **相关 Issue**：例如使用 `fixs #123` 关联对应 Issue或在github手动关联。
  - **测试情况**：说明你进行了哪些测试。
  - **截图或日志**（如有 UI 或日志变更）。

### 5. 代码审查与合并

- 提交 PR 后，至少需要一名维护者审查。
- 持续集成（CI）会自动运行测试和检查，请确保所有流水线通过。
- 根据反馈进行修改：在本地的同一分支上继续提交并推送，PR 会自动更新。
- 合并后，你的提交将出现在下一次版本发布的变更日志中（由维护者执行 `git-cliff` 生成）。

---

## 发布流程（仅维护者）

当收集到足够的变更后，维护者会执行以下步骤发布新版本：

1. 更新 `Cargo.toml` 中的版本号。
2. 运行 `git cliff --unreleased --tag x.x.x --prepend CHANGELOG.md` 更新变更日志。
3. 提交并打标签：`git tag vX.X.X`。
4. 推送并触发 GitHub Release。

---

## 获取帮助

- 如有疑问，请在相关 Issue 下留言。
- 也欢迎通过邮件（维护者邮箱）联系。

再次感谢你的贡献！

---
