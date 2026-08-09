# Contributing

Thanks for helping improve Config Editor.

## Development

Config Editor requires Go 1.25 and targets Linux/WSL.

```bash
go mod download
go test ./...
go test -race ./...
go vet ./...
go build ./cmd/config-editor
```

Run `gofmt` on changed Go files. Keep adapters inside the current user-configuration safety boundary and add tests for discovery, parsing, redaction and write behavior.

## Pull requests

- Explain the user-facing behavior and why the change is needed.
- Include tests for bug fixes and new behavior.
- Keep unrelated refactors in separate pull requests.
- Update the README or design document when behavior or safety boundaries change.
