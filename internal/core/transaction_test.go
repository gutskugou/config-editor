package core

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func testManager(t *testing.T) (Manager, string) {
	t.Helper()
	root := t.TempDir()
	home := filepath.Join(root, "home")
	state := filepath.Join(home, ".local", "state")
	config := filepath.Join(home, ".config")
	for _, path := range []string{home, state, config} {
		if err := os.MkdirAll(path, 0o700); err != nil {
			t.Fatal(err)
		}
	}
	return Manager{Home: home, ConfigRoot: config, StateRoot: state}, home
}

func TestApplyAndRestore(t *testing.T) {
	manager, home := testManager(t)
	path := filepath.Join(home, ".gitconfig")
	if err := os.WriteFile(path, []byte("[user]\nname=before\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.Prepare(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(change.StagePath, []byte("[user]\nname=after\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Apply(change); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(path)
	if !strings.Contains(string(got), "after") {
		t.Fatalf("apply failed: %s", got)
	}
	restore, err := manager.PrepareRestore(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Apply(restore); err != nil {
		t.Fatal(err)
	}
	got, _ = os.ReadFile(path)
	if !strings.Contains(string(got), "before") {
		t.Fatalf("restore failed: %s", got)
	}
}

func TestConcurrentChangeIsRejected(t *testing.T) {
	manager, home := testManager(t)
	path := filepath.Join(home, ".gitconfig")
	if err := os.WriteFile(path, []byte("a=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.Prepare(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(change.StagePath, []byte("a=2\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("a=3\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Apply(change); err == nil {
		t.Fatal("concurrent modification was accepted")
	}
}

func TestOutsideHomeIsRejected(t *testing.T) {
	manager, _ := testManager(t)
	path := filepath.Join(t.TempDir(), "config")
	if err := os.WriteFile(path, []byte("a=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Prepare(path, "git"); err == nil {
		t.Fatal("outside path was accepted")
	}
}

func TestSameContentReplacementIsRejected(t *testing.T) {
	manager, home := testManager(t)
	path := filepath.Join(home, ".gitconfig")
	before := []byte("a=1\n")
	if err := os.WriteFile(path, before, 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.Prepare(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(change.StagePath, []byte("a=2\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	replacement := filepath.Join(home, "replacement")
	if err := os.WriteFile(replacement, before, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(replacement, path); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Apply(change); err == nil || !strings.Contains(err.Error(), "replaced") {
		t.Fatalf("same-content replacement was not rejected: %v", err)
	}
}

func TestCorruptSnapshotIsRejected(t *testing.T) {
	manager, home := testManager(t)
	path := filepath.Join(home, ".gitconfig")
	if err := os.WriteFile(path, []byte("a=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.Prepare(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(change.StagePath, []byte("a=2\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	result, err := manager.Apply(change)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(result.Snapshot.ContentPath, []byte("corrupt\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.PrepareRestore(path, "git"); err == nil || !strings.Contains(err.Error(), "integrity") {
		t.Fatalf("corrupt snapshot was not rejected: %v", err)
	}
}

func TestApplyReportsPostCommitSyncWarning(t *testing.T) {
	manager, home := testManager(t)
	path := filepath.Join(home, ".gitconfig")
	if err := os.WriteFile(path, []byte("a=1\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	change, err := manager.Prepare(path, "git")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(change.StagePath, []byte("a=2\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	previous := syncDirectory
	syncDirectory = func(string) error { return errors.New("unsupported") }
	t.Cleanup(func() { syncDirectory = previous })
	result, err := manager.Apply(change)
	if err != nil {
		t.Fatalf("post-commit warning returned as failure: %v", err)
	}
	if result.Warning == nil {
		t.Fatal("post-commit sync warning was lost")
	}
	got, err := os.ReadFile(path)
	if err != nil || string(got) != "a=2\n" {
		t.Fatalf("committed content missing: %q, %v", got, err)
	}
}

func TestSimpleDiffShowsEndOfFileNewlineChange(t *testing.T) {
	diff := SimpleDiff([]byte("value"), []byte("value\n"))
	if strings.Contains(diff, "(no changes)") || !strings.Contains(diff, "end-of-file newline") {
		t.Fatalf("newline-only change was hidden:\n%s", diff)
	}
}
