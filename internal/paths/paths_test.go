package paths

import (
	"path/filepath"
	"testing"
)

func TestResolveHonorsAbsoluteXDGPaths(t *testing.T) {
	root := t.TempDir()
	t.Setenv("HOME", root)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(root, "cfg"))
	t.Setenv("XDG_STATE_HOME", filepath.Join(root, "state"))
	t.Setenv("XDG_CACHE_HOME", filepath.Join(root, "cache"))
	got, err := Resolve()
	if err != nil {
		t.Fatal(err)
	}
	if got.Config != filepath.Join(root, "cfg") || got.State != filepath.Join(root, "state") || got.Cache != filepath.Join(root, "cache") {
		t.Fatalf("unexpected paths: %#v", got)
	}
}
