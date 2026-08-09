# Changelog

All notable changes to Config Editor are documented in this file.

The project follows [Semantic Versioning](https://semver.org/).

## [0.2.1] - 2026-08-10

### Fixed

- 结构化编辑不再使用扫描时的旧行号，改为按稳定标识（key + 出现序号 + 原值）在当前内容中重新定位；同名键、重复 Host 的定位与歧义拒绝。
- 同一秒内创建多个快照时，按纳秒时间戳目录名稳定选取最新快照。
- Diff 滚动到底部后继续下滚不再累积不可见偏移，按一次向上立即生效。
- 暂存文件固定为 `0600` 私有权限，不继承源配置权限。
- Release 构建注入 commit SHA 与 UTC 构建时间。
- 详情视口扣除标题与应用列表行数，使用设置的实际显示行计算滚动窗口，小终端底部设置不再被裁剪。

### Added

- Release 归档附带 README、LICENSE 与 CHANGELOG。
- Release 流程校验 Git 标签与 Cargo.toml 版本一致。

## [0.2.0] - 2026-08-10

### Changed

- 使用 Rust 完全重写，功能与 v0.1.0 对齐；TUI 基于 ratatui。

## [0.1.0] - 2026-08-09

### Added

- Terminal UI for discovering and reviewing 12 common developer-tool configurations.
- Structured editing for Git, SSH, Starship, npm and pip settings.
- Staged `$VISUAL`/`$EDITOR` workflow with unified diff confirmation.
- Syntax checks for Bash, Zsh, Fish, JSONC and TOML.
- SHA-256 concurrency checks, private snapshots, atomic replacement and restoration.
- English and Chinese UI selected from `LANG`.
- `doctor`, `scan --json` and `version` commands.

### Security

- Restrict writes to existing regular files owned by the current user under approved configuration roots.
- Reject symlink swaps, multiple hard links, concurrent replacements and corrupted snapshots.
- Redact likely credentials from structured output.

[0.2.1]: https://github.com/gutskugou/config-editor/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/gutskugou/config-editor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gutskugou/config-editor/releases/tag/v0.1.0
