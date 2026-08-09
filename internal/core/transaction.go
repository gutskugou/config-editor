package core

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/gutskugou/config-editor/internal/adapters"
	"github.com/pmezard/go-difflib/difflib"
	"golang.org/x/sys/unix"
)

type Manager struct {
	Home, ConfigRoot, StateRoot string
}

type Change struct {
	DisplayPath string
	TargetPath  string
	StagePath   string
	Before      []byte
	BaseHash    string
	Mode        fs.FileMode
	Format      string
	Identity    fileIdentity
}

type fileIdentity struct {
	Device uint64
	Inode  uint64
}

type ApplyResult struct {
	Snapshot Snapshot
	Warning  error
}

type Snapshot struct {
	Path         string    `json:"path"`
	OriginalPath string    `json:"original_path"`
	ContentPath  string    `json:"content_path"`
	CreatedAt    time.Time `json:"created_at"`
	Hash         string    `json:"hash"`
}

func (m Manager) Prepare(path, format string) (*Change, error) {
	display, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}
	target, err := filepath.EvalSymlinks(display)
	if err != nil {
		return nil, fmt.Errorf("resolve target: %w", err)
	}
	if err := m.allowed(target); err != nil {
		return nil, err
	}
	file, info, err := openRegular(target)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	if err := validateInfo(info); err != nil {
		return nil, err
	}
	before, err := io.ReadAll(file)
	if err != nil {
		return nil, err
	}
	stageDir := filepath.Join(m.StateRoot, "config-editor", "edit")
	if err := secureDir(stageDir); err != nil {
		return nil, err
	}
	stage, err := os.CreateTemp(stageDir, "stage-*")
	if err != nil {
		return nil, err
	}
	stagePath := stage.Name()
	clean := false
	defer func() {
		if !clean {
			os.Remove(stagePath)
		}
	}()
	if err := stage.Chmod(info.Mode().Perm()); err != nil {
		stage.Close()
		return nil, err
	}
	if _, err := stage.Write(before); err != nil {
		stage.Close()
		return nil, err
	}
	if err := stage.Sync(); err != nil {
		stage.Close()
		return nil, err
	}
	if err := stage.Close(); err != nil {
		return nil, err
	}
	clean = true
	return &Change{DisplayPath: display, TargetPath: target, StagePath: stagePath, Before: before, BaseHash: digest(before), Mode: info.Mode().Perm(), Format: format, Identity: identity(info)}, nil
}

func (m Manager) Apply(change *Change) (ApplyResult, error) {
	defer os.Remove(change.StagePath)
	if err := m.allowed(change.TargetPath); err != nil {
		return ApplyResult{}, err
	}
	currentFile, info, err := openRegular(change.TargetPath)
	if err != nil {
		return ApplyResult{}, err
	}
	if err := validateInfo(info); err != nil {
		currentFile.Close()
		return ApplyResult{}, err
	}
	if identity(info) != change.Identity {
		currentFile.Close()
		return ApplyResult{}, errors.New("configuration file was replaced since editing began; nothing was written")
	}
	current, err := io.ReadAll(currentFile)
	closeErr := currentFile.Close()
	if err != nil {
		return ApplyResult{}, err
	}
	if closeErr != nil {
		return ApplyResult{}, closeErr
	}
	if digest(current) != change.BaseHash {
		return ApplyResult{}, errors.New("configuration changed since editing began; nothing was written")
	}
	after, err := readStage(m.StateRoot, change.StagePath)
	if err != nil {
		return ApplyResult{}, err
	}
	if err := adapters.Validate(change.Format, change.StagePath, after); err != nil {
		return ApplyResult{}, err
	}
	snapshot, err := m.snapshot(change.TargetPath, current)
	if err != nil {
		return ApplyResult{}, err
	}
	committed, err := atomicWrite(change.TargetPath, after, change.Mode, change.Identity)
	if err != nil {
		if committed {
			return ApplyResult{Snapshot: snapshot, Warning: fmt.Errorf("change was applied but directory sync failed: %w", err)}, nil
		}
		_ = os.RemoveAll(snapshot.Path)
		return ApplyResult{}, err
	}
	return ApplyResult{Snapshot: snapshot}, nil
}

func (m Manager) Discard(change *Change) error { return os.Remove(change.StagePath) }

func (m Manager) Latest(path string) (Snapshot, error) {
	dir := filepath.Join(m.StateRoot, "config-editor", "snapshots")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return Snapshot{}, err
	}
	var latest Snapshot
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		metaPath := filepath.Join(dir, entry.Name(), "metadata.json")
		data, err := os.ReadFile(metaPath)
		if err != nil {
			continue
		}
		var candidate Snapshot
		if json.Unmarshal(data, &candidate) != nil || candidate.OriginalPath != path {
			continue
		}
		candidate.Path = filepath.Dir(metaPath)
		candidate.ContentPath = filepath.Join(candidate.Path, "content")
		if candidate.CreatedAt.After(latest.CreatedAt) {
			latest = candidate
		}
	}
	if latest.Path == "" {
		return Snapshot{}, errors.New("no snapshot found for this file")
	}
	return latest, nil
}

func (m Manager) PrepareRestore(path, format string) (*Change, error) {
	change, err := m.Prepare(path, format)
	if err != nil {
		return nil, err
	}
	snapshot, err := m.Latest(change.TargetPath)
	if err != nil {
		m.Discard(change)
		return nil, err
	}
	content, err := os.ReadFile(snapshot.ContentPath)
	if err != nil {
		m.Discard(change)
		return nil, err
	}
	if snapshot.Hash == "" || digest(content) != snapshot.Hash {
		m.Discard(change)
		return nil, errors.New("snapshot content failed its SHA-256 integrity check")
	}
	if err := os.WriteFile(change.StagePath, content, change.Mode); err != nil {
		m.Discard(change)
		return nil, err
	}
	return change, nil
}

func (m Manager) snapshot(path string, content []byte) (Snapshot, error) {
	root := filepath.Join(m.StateRoot, "config-editor", "snapshots")
	if err := secureDir(root); err != nil {
		return Snapshot{}, err
	}
	now := time.Now().UTC()
	name := now.Format("20060102T150405.000000000Z") + "-" + digest([]byte(path))[:10]
	dir := filepath.Join(root, name)
	if err := os.Mkdir(dir, 0o700); err != nil {
		return Snapshot{}, err
	}
	complete := false
	defer func() {
		if !complete {
			_ = os.RemoveAll(dir)
		}
	}()
	contentPath := filepath.Join(dir, "content")
	if err := os.WriteFile(contentPath, content, 0o600); err != nil {
		return Snapshot{}, err
	}
	snapshot := Snapshot{Path: dir, OriginalPath: path, ContentPath: contentPath, CreatedAt: now, Hash: digest(content)}
	meta, err := json.MarshalIndent(snapshot, "", "  ")
	if err != nil {
		return Snapshot{}, err
	}
	if err := os.WriteFile(filepath.Join(dir, "metadata.json"), meta, 0o600); err != nil {
		return Snapshot{}, err
	}
	complete = true
	return snapshot, nil
}

func (m Manager) allowed(target string) error {
	for _, root := range []string{m.Home, m.ConfigRoot} {
		root, err := filepath.Abs(root)
		if err != nil {
			continue
		}
		rel, err := filepath.Rel(root, target)
		if err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
			return nil
		}
	}
	return fmt.Errorf("refusing to edit outside the user configuration roots: %s", target)
}

func atomicWrite(path string, content []byte, mode fs.FileMode, expected fileIdentity) (bool, error) {
	dir := filepath.Dir(path)
	temp, err := os.CreateTemp(dir, ".config-editor-*")
	if err != nil {
		return false, err
	}
	tempPath := temp.Name()
	defer os.Remove(tempPath)
	if err := temp.Chmod(mode.Perm()); err != nil {
		temp.Close()
		return false, err
	}
	if _, err := temp.Write(content); err != nil {
		temp.Close()
		return false, err
	}
	if err := temp.Sync(); err != nil {
		temp.Close()
		return false, err
	}
	if err := temp.Close(); err != nil {
		return false, err
	}
	current, info, err := openRegular(path)
	if err != nil {
		return false, err
	}
	current.Close()
	if err := validateInfo(info); err != nil {
		return false, err
	}
	if identity(info) != expected {
		return false, errors.New("configuration file was replaced before commit; nothing was written")
	}
	if err := os.Rename(tempPath, path); err != nil {
		return false, err
	}
	return true, syncDirectory(dir)
}

var syncDirectory = func(dir string) error {
	d, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer d.Close()
	return d.Sync()
}

func openRegular(path string) (*os.File, fs.FileInfo, error) {
	fd, err := unix.Open(path, unix.O_RDONLY|unix.O_CLOEXEC|unix.O_NOFOLLOW, 0)
	if err != nil {
		return nil, nil, err
	}
	file := os.NewFile(uintptr(fd), path)
	info, err := file.Stat()
	if err != nil {
		file.Close()
		return nil, nil, err
	}
	if !info.Mode().IsRegular() {
		file.Close()
		return nil, nil, errors.New("only regular files are supported")
	}
	return file, info, nil
}

func validateInfo(info fs.FileInfo) error {
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok {
		return errors.New("cannot verify configuration ownership")
	}
	if stat.Uid != uint32(os.Getuid()) {
		return errors.New("configuration is not owned by the current user")
	}
	if stat.Nlink > 1 {
		return errors.New("files with multiple hard links are not edited safely")
	}
	return nil
}

func identity(info fs.FileInfo) fileIdentity {
	stat, _ := info.Sys().(*syscall.Stat_t)
	if stat == nil {
		return fileIdentity{}
	}
	return fileIdentity{Device: uint64(stat.Dev), Inode: stat.Ino}
}

func secureDir(path string) error {
	if err := os.MkdirAll(path, 0o700); err != nil {
		return err
	}
	return os.Chmod(path, 0o700)
}

func readStage(stateRoot, path string) ([]byte, error) {
	root := filepath.Join(stateRoot, "config-editor", "edit")
	rel, err := filepath.Rel(root, path)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return nil, errors.New("staged file is outside the private edit directory")
	}
	file, _, err := openRegular(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	return io.ReadAll(file)
}

func digest(content []byte) string { sum := sha256.Sum256(content); return hex.EncodeToString(sum[:]) }

func SimpleDiff(before, after []byte) string {
	if bytes.Equal(before, after) {
		return "--- current\n+++ proposed\n(no changes)\n"
	}
	diff, err := difflib.GetUnifiedDiffString(difflib.UnifiedDiff{
		A:        difflib.SplitLines(string(before)),
		B:        difflib.SplitLines(string(after)),
		FromFile: "current",
		ToFile:   "proposed",
		Context:  3,
		Eol:      "\n",
	})
	if err != nil {
		return "--- current\n+++ proposed\n(diff unavailable: " + sanitize(err.Error()) + ")\n"
	}
	if diff == "" {
		return "--- current\n+++ proposed\n(end-of-file newline changed)\n"
	}
	beforeNewline := bytes.HasSuffix(before, []byte("\n"))
	afterNewline := bytes.HasSuffix(after, []byte("\n"))
	if beforeNewline != afterNewline {
		change := "removed"
		if afterNewline {
			change = "added"
		}
		diff += "(end-of-file newline " + change + ")\n"
	}
	return sanitize(diff)
}

func sanitize(value string) string {
	return strings.Map(func(r rune) rune {
		if r == '\n' || r == '\t' || r >= 32 {
			return r
		}
		return '�'
	}, value)
}
