package main

import (
	"strings"
	"testing"
)

func TestVersionString(t *testing.T) {
	previousVersion, previousCommit, previousDate := version, commit, date
	t.Cleanup(func() { version, commit, date = previousVersion, previousCommit, previousDate })
	version, commit, date = "v0.1.0", "abc123", "2026-08-09T00:00:00Z"
	got := versionString()
	for _, want := range []string{"config-editor v0.1.0", "abc123", "2026-08-09"} {
		if !strings.Contains(got, want) {
			t.Fatalf("version output %q does not contain %q", got, want)
		}
	}
}
