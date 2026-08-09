# Changelog

All notable changes to Config Editor are documented in this file.

The project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/gutskugou/config-editor/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gutskugou/config-editor/releases/tag/v0.1.0
