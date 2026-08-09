# Config Editor TUI MVP

## Goal

Make personal Linux configuration changes explainable and reversible without replacing the files that existing programs already understand. The files remain the source of truth; Config Editor is an index, parser and safe transaction layer.

## User loop

1. Discover 12 supported applications and their conventional user-level paths.
2. Inspect files and non-sensitive structured settings in one terminal interface.
3. Change one supported value or open a temporary staged copy in the user's editor.
4. Review a terminal-sized, scrollable unified text diff.
5. Validate supported syntax.
6. Re-check the original file hash, create a private snapshot, and atomically replace it.
7. Prepare a previous snapshot through the same diff/confirmation path when recovery is needed.

## Safety boundary

The MVP only writes existing regular files owned by the current UID under HOME or XDG_CONFIG_HOME. Symlinks are resolved before boundary checks. Multiple hard links and targets outside the allowed roots are rejected. It does not elevate privileges.

The staged file lives under XDG_STATE_HOME/config-editor/edit; snapshots live under XDG_STATE_HOME/config-editor/snapshots with mode 0700 directories and 0600 content/metadata. Before committing, the tool re-checks the live file identity, ownership, link count and SHA-256 captured at staging time. The final write uses a temporary file in the destination directory, fsync, and rename. A directory-sync failure after rename is reported as an applied change with a durability warning, never as an uncommitted failure. Snapshot content is hash-checked before restoration.

## Adapter levels

- Discovery: all 12 applications.
- Structured view/edit: Git, SSH, Starship, npm and pip, limited to stable line-oriented settings.
- Syntax validation: Bash, Zsh and Fish through their -n modes; JSONC and TOML through parsers.
- Staged text editing only: tmux, Vim and Neovim; the MVP deliberately does not execute user configuration as a validator.

Potential secrets such as tokens, passwords and authentication fields are redacted and cannot be edited through the structured view. The raw staged editor remains available because it preserves user agency without teaching the tool to store secrets.

## Evidence of usefulness

The MVP is verifiable through JSON discovery output, doctor diagnostics, unit tests for safety properties, a TUI render smoke test, and CI on Ubuntu 24.04.

The next product decision should be based on observed sessions: discovery hit rate, whether users understand the proposed diff, validation failures caught before writes, successful restores, and unsupported-file requests. No telemetry is built into this local MVP.
