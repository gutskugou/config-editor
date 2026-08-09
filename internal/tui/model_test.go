package tui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/gutskugou/config-editor/internal/core"
	"github.com/gutskugou/config-editor/internal/domain"
	"github.com/gutskugou/config-editor/internal/i18n"
)

func press(model Model, key tea.Key) Model {
	next, _ := model.Update(tea.KeyPressMsg(key))
	return next.(Model)
}

func TestRenderSmoke(t *testing.T) {
	apps := []domain.Application{{ID: "git", Name: "Git", NameZH: "Git", Installed: true, Sources: []domain.Source{{Path: "/home/me/.gitconfig", Exists: true, Settings: []domain.Setting{{Key: "user.name", Value: "Ada", Editable: true}}}}}}
	output := New(apps, core.Manager{}, i18n.Catalog{}).render()
	for _, want := range []string{"Config Editor", "Git", "user.name", "Ada"} {
		if !strings.Contains(output, want) {
			t.Fatalf("render missing %q:\n%s", want, output)
		}
	}
}

func TestNavigationKeysMoveApplicationsAndSettings(t *testing.T) {
	apps := []domain.Application{
		{ID: "one", Name: "One", Sources: []domain.Source{{Settings: []domain.Setting{{Key: "a"}, {Key: "b"}}}}},
		{ID: "two", Name: "Two"},
	}
	model := New(apps, core.Manager{}, i18n.Catalog{})
	model = press(model, tea.Key{Code: 'j', Text: "j"})
	if model.appIndex != 1 {
		t.Fatalf("j did not move application: %d", model.appIndex)
	}
	model = press(model, tea.Key{Code: tea.KeyUp})
	if model.appIndex != 0 {
		t.Fatalf("up did not move application: %d", model.appIndex)
	}
	model = press(model, tea.Key{Code: tea.KeyRight})
	model = press(model, tea.Key{Code: tea.KeyDown})
	if model.focus != settingsPane || model.settingIndex != 1 {
		t.Fatalf("down did not move setting: focus=%d index=%d", model.focus, model.settingIndex)
	}
}

func TestEmptySettingsPaneDoesNotTakeFocus(t *testing.T) {
	model := New([]domain.Application{{ID: "bash", Name: "Bash"}}, core.Manager{}, i18n.Catalog{})
	model = press(model, tea.Key{Code: tea.KeyRight})
	if model.focus != appsPane || !strings.Contains(model.status, "No structured settings") {
		t.Fatalf("empty settings pane took focus: focus=%d status=%q", model.focus, model.status)
	}
}

func TestUnicodeInputBackspaceAndPaste(t *testing.T) {
	model := New(nil, core.Manager{}, i18n.Catalog{})
	model.prompt, model.input = searchPrompt, "中文"
	model = press(model, tea.Key{Code: tea.KeyBackspace})
	if model.input != "中" {
		t.Fatalf("unicode backspace corrupted input: %q", model.input)
	}
	model = press(model, tea.Key{Text: "配置"})
	if model.input != "中配置" {
		t.Fatalf("multi-rune paste was not accepted: %q", model.input)
	}
}

func TestRefreshReadsThroughResolvedTarget(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "target")
	if err := os.WriteFile(target, []byte("[user]\nname = \"Alice\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(dir, "link")
	if err := os.Symlink(target, link); err != nil {
		t.Fatal(err)
	}
	apps := []domain.Application{{ID: "git", Capabilities: []domain.Capability{domain.CapabilityStructured}, Sources: []domain.Source{{Path: link, Resolved: target, Exists: true, Format: "git"}}}}
	model := New(apps, core.Manager{}, i18n.Catalog{})
	if err := os.Remove(link); err != nil {
		t.Fatal(err)
	}
	model.refresh()
	settings := model.apps[0].Sources[0].Settings
	if len(settings) != 1 || settings[0].Key != "user.name" || settings[0].Value != `"Alice"` {
		t.Fatalf("refresh did not read through resolved target: %#v", settings)
	}
}

func TestRenderAndDiffRespectTerminalHeight(t *testing.T) {
	var apps []domain.Application
	for i := 0; i < 12; i++ {
		apps = append(apps, domain.Application{ID: fmt.Sprint(i), Name: fmt.Sprintf("App %d", i), Sources: []domain.Source{{Path: "/a/very/long/path", Settings: []domain.Setting{{Key: "key", Value: strings.Repeat("值", 40)}}}}})
	}
	model := New(apps, core.Manager{}, i18n.Catalog{})
	model.width, model.height = 40, 12
	if lines := strings.Count(model.render(), "\n") + 1; lines > model.height {
		t.Fatalf("normal view has %d lines, height is %d", lines, model.height)
	}
	var diff strings.Builder
	for i := 0; i < 30; i++ {
		fmt.Fprintf(&diff, "+line %d\n", i)
	}
	model.prompt, model.diff = confirmPrompt, diff.String()
	if lines := strings.Count(model.render(), "\n") + 1; lines > model.height {
		t.Fatalf("diff view has %d lines, height is %d", lines, model.height)
	}
	model = press(model, tea.Key{Code: tea.KeyDown})
	if model.diffOffset != 1 {
		t.Fatalf("diff did not scroll: %d", model.diffOffset)
	}
}
