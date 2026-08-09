# Config Editor

[![CI](https://github.com/gutskugou/config-editor/actions/workflows/ci.yml/badge.svg)](https://github.com/gutskugou/config-editor/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/gutskugou/config-editor)](https://github.com/gutskugou/config-editor/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

一个面向个人 Linux/WSL 环境的安全配置工具。它集中发现常见开发工具的配置，并把高风险的直接写入变成可暂存、可审阅、可验证和可恢复的操作。

Config Editor is a safety layer for personal Linux configuration files. It discovers familiar developer tools and turns risky direct edits into staged, reviewable, validated and recoverable changes.

> **MVP status:** v0.2.0 只管理当前用户已有的配置文件，不管理 `/etc`、服务、软件包、远程机器或 PowerShell `$PROFILE`。

## 功能 / Features

- 发现 12 类配置：Git、SSH client、Bash、Zsh、Fish、tmux、Vim、Neovim、VS Code、Starship、npm 和 pip。
- Git、SSH、Starship、npm 和 pip 支持有限的逐行结构化编辑。
- 所有已存在的配置文件都可以通过 `$VISUAL`/`$EDITOR` 编辑私有暂存副本。
- Bash、Zsh、Fish、JSONC 和 TOML 在适用时执行语法检查。
- 每次写入前展示可滚动 unified diff，并要求明确确认。
- 使用 SHA-256 并发检查、私有快照、同目录原子替换和恢复流程。
- 根据 `LANG` 自动选择中文或英文 TUI。
- 永不使用 `sudo`，拒绝写入用户 HOME/XDG 配置根目录之外的目标。

## 安装 / Installation

### GitHub Release

从 [Releases](https://github.com/gutskugou/config-editor/releases) 下载与你的 Linux 架构对应的压缩包：

```bash
VERSION=0.2.0
ARCH=amd64 # arm64 is also available
curl -LO "https://github.com/gutskugou/config-editor/releases/download/v${VERSION}/config-editor_${VERSION}_linux_${ARCH}.tar.gz"
curl -LO "https://github.com/gutskugou/config-editor/releases/download/v${VERSION}/checksums.txt"
sha256sum --ignore-missing -c checksums.txt
tar -xzf "config-editor_${VERSION}_linux_${ARCH}.tar.gz"
install -Dm755 config-editor ~/.local/bin/config-editor
config-editor version
```

确保 `~/.local/bin` 已加入 `PATH`。

### 从源码构建 / Build from source

需要 Rust 1.97+：

```bash
git clone https://github.com/gutskugou/config-editor.git
cd config-editor
cargo build --release
install -Dm755 target/release/config-editor ~/.local/bin/config-editor
```

## 使用 / Usage

```bash
config-editor doctor       # 检查环境和可用工具
config-editor scan --json  # 输出发现结果，不修改文件
config-editor version      # 输出版本和构建信息
config-editor              # 启动 TUI
```

TUI 快捷键：

| Key | Action |
|---|---|
| `↑`/`↓`, `j`/`k` | 移动或滚动 Diff |
| `→`, `l`, `Enter` | 进入结构化设置 |
| `←`, `h`, `Esc` | 返回应用列表 |
| `s` | 修改选中的结构化值 |
| `e` | 使用编辑器修改暂存副本 |
| `r` | 准备恢复最新快照 |
| `/` | 搜索应用 |
| `y` / `n` | 应用或放弃待确认变更 |
| `q` | 退出 |

### 从 Windows PowerShell 启动 WSL

先进入 WSL shell，再运行 TUI，以确保终端输入正确传递：

```powershell
wsl.exe --cd ~
```

然后在 WSL 内运行：

```bash
config-editor
```

## 安全边界 / Safety boundary

- 只写入当前 UID 拥有、已经存在的普通文件。
- 目标必须位于 `HOME` 或 `XDG_CONFIG_HOME` 下；符号链接在边界检查前解析。
- 多硬链接、文件替换、并发内容变化和损坏快照都会被拒绝。
- 暂存内容位于 `XDG_STATE_HOME/config-editor/edit`；快照位于 `XDG_STATE_HOME/config-editor/snapshots`。
- 可能包含密码、token 或 URL 凭据的结构化值会被隐藏并禁止行内编辑。
- tmux、Vim 和 Neovim 配置只暂存和审阅，不执行用户配置进行验证。

复杂或包含秘密的内容仍应通过暂存编辑器处理，并在确认前仔细检查 Diff。

## 开发 / Development

支持目标为 Ubuntu 24.04 和 WSL，构建需要 Rust 1.97+：

```bash
git clone https://github.com/gutskugou/config-editor.git
cd config-editor
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
./target/release/config-editor
```

更多信息：

- [贡献指南](CONTRIBUTING.md)
- [安全策略](SECURITY.md)

## English summary

Config Editor targets personal Linux and WSL environments. It discovers conventional user-level configuration files, exposes a small set of non-sensitive line-oriented settings, and routes every write through staging, review, validation, snapshotting and atomic replacement.

The v0.2.0 release intentionally does not manage system configuration, privilege escalation, services, package installation, remote machines or native Windows profiles. See the sections above for installation, commands, key bindings and safety details.

## License

[MIT](LICENSE)
