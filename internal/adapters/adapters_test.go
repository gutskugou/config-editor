package adapters

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/gutskugou/config-editor/internal/domain"
	"github.com/gutskugou/config-editor/internal/paths"
)

func TestBuiltins(t *testing.T) {
	if got := Builtins(); got != 12 {
		t.Fatalf("Builtins() = %d, want 12", got)
	}
}

func TestScanDoesNotTreatShellAsStructuredSettings(t *testing.T) {
	home := t.TempDir()
	if err := os.WriteFile(filepath.Join(home, ".bashrc"), []byte("if [ \"$x\" = y ]; then\n  export A=b\nfi\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	apps, err := Scan(paths.Paths{Home: home, Config: filepath.Join(home, ".config"), State: filepath.Join(home, ".state")})
	if err != nil {
		t.Fatal(err)
	}
	for _, app := range apps {
		if app.ID == "bash" && len(app.Sources[0].Settings) != 0 {
			t.Fatalf("shell script exposed as structured settings: %#v", app.Sources[0].Settings)
		}
	}
}

func TestParseSettingsRedactsSecrets(t *testing.T) {
	input := []byte("[user]\nname = Ada\npassword = swordfish\n")
	settings := ParseSettings("git", input)
	if len(settings) != 2 {
		t.Fatalf("got %d settings", len(settings))
	}
	if settings[0].Key != "user.name" || settings[0].Value != "Ada" {
		t.Fatalf("unexpected setting: %#v", settings[0])
	}
	if !settings[1].Sensitive || settings[1].Editable || strings.Contains(settings[1].Value, "swordfish") {
		t.Fatalf("secret exposed: %#v", settings[1])
	}
}

func TestParseSettingsRedactsCredentialsInValues(t *testing.T) {
	input := []byte("[global]\nindex-url=https://user:swordfish@example.test/simple\nheader=Bearer abc.def.ghi\n")
	settings := ParseSettings("ini", input)
	if len(settings) != 2 {
		t.Fatalf("got %d settings", len(settings))
	}
	for _, setting := range settings {
		if !setting.Sensitive || setting.Editable || setting.Value != "••••••" {
			t.Fatalf("credential exposed: %#v", setting)
		}
	}
}

func TestParseSupportedLineFormats(t *testing.T) {
	pip := ParseSettings("ini", []byte("[global]\ntimeout: 30\n"))
	if len(pip) != 1 || pip[0].Key != "global.timeout" || pip[0].Value != "30" {
		t.Fatalf("pip colon setting not parsed: %#v", pip)
	}
	git := ParseSettings("git", []byte("[core]\nbare\n"))
	if len(git) != 1 || git[0].Key != "core.bare" || git[0].Value != "true" {
		t.Fatalf("git boolean setting not parsed: %#v", git)
	}
	ssh := ParseSettings("ssh", []byte("Host example\n  User ada\nHost other\n  User grace\n"))
	if len(ssh) != 2 || ssh[0].Key != "Host example.User" || ssh[1].Key != "Host other.User" {
		t.Fatalf("ssh scopes missing: %#v", ssh)
	}
}

func TestScanReportsSourceDiagnosticWithoutAborting(t *testing.T) {
	home := t.TempDir()
	if err := os.Mkdir(filepath.Join(home, ".gitconfig"), 0o700); err != nil {
		t.Fatal(err)
	}
	apps, err := Scan(paths.Paths{Home: home, Config: filepath.Join(home, ".config"), State: filepath.Join(home, ".state")})
	if err != nil {
		t.Fatal(err)
	}
	for _, app := range apps {
		if app.ID == "git" {
			if app.Sources[0].Diagnostic == "" {
				t.Fatal("non-regular source diagnostic was not reported")
			}
			return
		}
	}
	t.Fatal("git adapter missing")
}

func TestReplaceSettingPreservesOtherLines(t *testing.T) {
	before := []byte("# keep\n[user]\n\tname = Ada\n\temail = ada@example.test\n")
	setting := domain.Setting{Key: "user.name", Value: "Ada", Line: 3, Editable: true}
	after, err := ReplaceSetting("git", before, setting, "Grace")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(after), "# keep") || !strings.Contains(string(after), "email = ada@example.test") || !strings.Contains(string(after), "name=Grace") {
		t.Fatalf("unexpected replacement:\n%s", after)
	}
}

func TestReplaceBareGitBoolean(t *testing.T) {
	before := []byte("[core]\n\tbare\n")
	setting := domain.Setting{Key: "core.bare", Value: "true", Line: 2, Editable: true}
	after, err := ReplaceSetting("git", before, setting, "false")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(after), "\tbare=false") {
		t.Fatalf("bare boolean replacement failed:\n%s", after)
	}
}

func TestValidateJSONC(t *testing.T) {
	if err := Validate("jsonc", "settings.json", []byte("{ // comment\n \"x\": true,\n}")); err != nil {
		t.Fatal(err)
	}
	if err := Validate("jsonc", "settings.json", []byte("{ nope")); err == nil {
		t.Fatal("invalid JSONC accepted")
	}
}
