# Contributing

Thanks for helping improve Config Editor.

## Development

Config Editor requires Rust 1.97+ and targets Linux/WSL.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Run `cargo fmt` on changed Rust files and keep `cargo clippy -- -D warnings` free of warnings. Keep adapters inside the current user-configuration safety boundary and add tests for discovery, parsing, redaction and write behavior.

## Pull requests

- Explain the user-facing behavior and why the change is needed.
- Include tests for bug fixes and new behavior.
- Keep unrelated refactors in separate pull requests.
- Update the README or design document when behavior or safety boundaries change.
