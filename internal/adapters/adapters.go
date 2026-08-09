package adapters

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/gutskugou/config-editor/internal/domain"
	"github.com/gutskugou/config-editor/internal/paths"
	"github.com/pelletier/go-toml/v2"
	"github.com/tidwall/jsonc"
)

type candidate struct {
	path   func(paths.Paths) string
	format string
}

type definition struct {
	id, name, nameZH, description, descriptionZH, command string
	capabilities                                          []domain.Capability
	candidates                                            []candidate
}

func Builtins() int { return len(definitions()) }

func definitions() []definition {
	c := func(format string, fn func(paths.Paths) string) candidate { return candidate{fn, format} }
	structured := []domain.Capability{domain.CapabilityDiscover, domain.CapabilityStructured, domain.CapabilityRaw}
	return []definition{
		{"git", "Git", "Git", "Identity, aliases and repository defaults", "身份、别名和仓库默认值", "git", structured, []candidate{c("git", func(p paths.Paths) string { return filepath.Join(p.Home, ".gitconfig") }), c("git", func(p paths.Paths) string { return filepath.Join(p.Config, "git", "config") })}},
		{"ssh", "SSH client", "SSH 客户端", "Hosts, users, ports and keys", "主机、用户、端口和密钥", "ssh", structured, []candidate{c("ssh", func(p paths.Paths) string { return filepath.Join(p.Home, ".ssh", "config") })}},
		{"bash", "Bash", "Bash", "Interactive shell startup", "交互式 Shell 启动配置", "bash", []domain.Capability{domain.CapabilityDiscover, domain.CapabilitySyntax, domain.CapabilityRaw}, []candidate{c("bash", func(p paths.Paths) string { return filepath.Join(p.Home, ".bashrc") }), c("bash", func(p paths.Paths) string { return filepath.Join(p.Home, ".bash_profile") }), c("bash", func(p paths.Paths) string { return filepath.Join(p.Home, ".profile") })}},
		{"zsh", "Zsh", "Zsh", "Interactive shell startup", "交互式 Shell 启动配置", "zsh", []domain.Capability{domain.CapabilityDiscover, domain.CapabilitySyntax, domain.CapabilityRaw}, []candidate{c("zsh", func(p paths.Paths) string { return filepath.Join(p.Home, ".zshrc") }), c("zsh", func(p paths.Paths) string { return filepath.Join(p.Config, "zsh", ".zshrc") })}},
		{"fish", "Fish", "Fish", "Interactive shell startup", "交互式 Shell 启动配置", "fish", []domain.Capability{domain.CapabilityDiscover, domain.CapabilitySyntax, domain.CapabilityRaw}, []candidate{c("fish", func(p paths.Paths) string { return filepath.Join(p.Config, "fish", "config.fish") })}},
		{"tmux", "tmux", "tmux", "Terminal multiplexer preferences", "终端复用器偏好", "tmux", []domain.Capability{domain.CapabilityDiscover, domain.CapabilityRaw}, []candidate{c("tmux", func(p paths.Paths) string { return filepath.Join(p.Home, ".tmux.conf") }), c("tmux", func(p paths.Paths) string { return filepath.Join(p.Config, "tmux", "tmux.conf") })}},
		{"vim", "Vim", "Vim", "Editor preferences and plugins", "编辑器偏好和插件", "vim", []domain.Capability{domain.CapabilityDiscover, domain.CapabilityRaw}, []candidate{c("vim", func(p paths.Paths) string { return filepath.Join(p.Home, ".vimrc") }), c("vim", func(p paths.Paths) string { return filepath.Join(p.Config, "vim", "vimrc") })}},
		{"nvim", "Neovim", "Neovim", "Editor preferences and plugins", "编辑器偏好和插件", "nvim", []domain.Capability{domain.CapabilityDiscover, domain.CapabilityRaw}, []candidate{c("lua", func(p paths.Paths) string { return filepath.Join(p.Config, "nvim", "init.lua") }), c("vim", func(p paths.Paths) string { return filepath.Join(p.Config, "nvim", "init.vim") })}},
		{"vscode", "VS Code Remote", "VS Code 远程", "Remote user settings", "远程用户设置", "code", []domain.Capability{domain.CapabilityDiscover, domain.CapabilitySyntax, domain.CapabilityRaw}, []candidate{c("jsonc", func(p paths.Paths) string {
			return filepath.Join(p.Home, ".vscode-server", "data", "Machine", "settings.json")
		}), c("jsonc", func(p paths.Paths) string { return filepath.Join(p.Config, "Code", "User", "settings.json") })}},
		{"starship", "Starship", "Starship", "Cross-shell prompt settings", "跨 Shell 提示符设置", "starship", append(structured, domain.CapabilitySyntax), []candidate{c("toml", func(p paths.Paths) string { return filepath.Join(p.Config, "starship.toml") })}},
		{"npm", "npm", "npm", "Package manager defaults", "包管理器默认值", "npm", structured, []candidate{c("properties", func(p paths.Paths) string { return filepath.Join(p.Home, ".npmrc") }), c("properties", func(p paths.Paths) string { return filepath.Join(p.Config, "npm", "npmrc") })}},
		{"pip", "pip", "pip", "Python package installer defaults", "Python 包安装器默认值", "pip", structured, []candidate{c("ini", func(p paths.Paths) string { return filepath.Join(p.Config, "pip", "pip.conf") }), c("ini", func(p paths.Paths) string { return filepath.Join(p.Home, ".pip", "pip.conf") })}},
	}
}

func Scan(p paths.Paths) ([]domain.Application, error) {
	apps := make([]domain.Application, 0, Builtins())
	for _, d := range definitions() {
		_, commandErr := exec.LookPath(d.command)
		a := domain.Application{ID: d.id, Name: d.name, NameZH: d.nameZH, Description: d.description, DescriptionZH: d.descriptionZH, Command: d.command, Installed: commandErr == nil, Capabilities: d.capabilities}
		for _, item := range d.candidates {
			path := filepath.Clean(item.path(p))
			source := domain.Source{Path: path, Format: item.format}
			info, err := os.Stat(path)
			if err == nil && info.Mode().IsRegular() {
				source.Exists = true
				if resolved, resolveErr := filepath.EvalSymlinks(path); resolveErr == nil {
					source.Resolved = resolved
				}
				content, readErr := os.ReadFile(path)
				if readErr != nil {
					source.Diagnostic = readErr.Error()
				} else if hasCapability(d.capabilities, domain.CapabilityStructured) {
					source.Settings = ParseSettings(item.format, content)
				}
			} else if err == nil {
				source.Diagnostic = "path exists but is not a regular file"
			} else if !errors.Is(err, os.ErrNotExist) {
				source.Diagnostic = fmt.Errorf("inspect: %w", err).Error()
			}
			a.Sources = append(a.Sources, source)
		}
		apps = append(apps, a)
	}
	sort.SliceStable(apps, func(i, j int) bool {
		if apps[i].Configured() != apps[j].Configured() {
			return apps[i].Configured()
		}
		if apps[i].Installed != apps[j].Installed {
			return apps[i].Installed
		}
		return strings.ToLower(apps[i].Name) < strings.ToLower(apps[j].Name)
	})
	return apps, nil
}

func hasCapability(capabilities []domain.Capability, wanted domain.Capability) bool {
	for _, capability := range capabilities {
		if capability == wanted {
			return true
		}
	}
	return false
}

var (
	sensitiveKey   = regexp.MustCompile(`(?i)(token|password|passwd|secret|private[_-]?key|api[_-]?key|access[_-]?key|credential|_auth)`)
	sensitiveValue = regexp.MustCompile(`(?i)\b(?:bearer|basic)\s+[a-z0-9._~+/=-]+|://[^/\s:@]+:[^@\s/]+@`)
)

func ParseSettings(format string, content []byte) []domain.Setting {
	var out []domain.Setting
	section := ""
	sshScope := ""
	for index, raw := range strings.Split(string(content), "\n") {
		line := strings.TrimSpace(raw)
		if line == "" || strings.HasPrefix(line, "#") || strings.HasPrefix(line, ";") {
			continue
		}
		if strings.HasPrefix(line, "[") && strings.HasSuffix(line, "]") {
			section = strings.TrimSpace(line[1 : len(line)-1])
			continue
		}
		key, value, ok := splitSetting(format, line)
		if !ok {
			continue
		}
		if format == "ssh" && (strings.EqualFold(key, "Host") || strings.EqualFold(key, "Match")) {
			sshScope = key + " " + value
			continue
		}
		if format == "ssh" && sshScope != "" {
			key = sshScope + "." + key
		} else if section != "" {
			key = section + "." + key
		}
		secret := isSensitive(key, value)
		if secret {
			value = "••••••"
		}
		out = append(out, domain.Setting{Key: key, Value: value, Line: index + 1, Editable: !secret, Sensitive: secret})
	}
	return out
}

func isSensitive(key, value string) bool {
	if sensitiveKey.MatchString(key) || sensitiveValue.MatchString(value) {
		return true
	}
	trimmed := strings.Trim(strings.TrimSpace(value), `"'`)
	parsed, err := url.Parse(trimmed)
	if err != nil || parsed.User == nil {
		return false
	}
	_, hasPassword := parsed.User.Password()
	return hasPassword
}

func splitSetting(format, line string) (string, string, bool) {
	if format == "ssh" {
		parts := strings.Fields(line)
		if len(parts) < 2 {
			return "", "", false
		}
		return parts[0], strings.Join(parts[1:], " "), true
	}
	if format == "ini" {
		pos := strings.IndexAny(line, "=:")
		if pos > 0 {
			return strings.TrimSpace(line[:pos]), strings.TrimSpace(line[pos+1:]), true
		}
		return "", "", false
	}
	if pos := strings.Index(line, "="); pos > 0 {
		return strings.TrimSpace(line[:pos]), strings.TrimSpace(line[pos+1:]), true
	}
	if format == "git" {
		parts := strings.Fields(line)
		if len(parts) == 1 {
			return parts[0], "true", true
		}
	}
	return "", "", false
}

func ReplaceSetting(format string, content []byte, setting domain.Setting, value string) ([]byte, error) {
	if !setting.Editable || setting.Sensitive {
		return nil, errors.New("sensitive settings cannot be edited here")
	}
	lines := strings.Split(string(content), "\n")
	if setting.Line < 1 || setting.Line > len(lines) {
		return nil, errors.New("setting line is no longer present")
	}
	line := lines[setting.Line-1]
	if format == "ssh" {
		indent := line[:len(line)-len(strings.TrimLeft(line, " \t"))]
		key := strings.Fields(strings.TrimSpace(line))[0]
		lines[setting.Line-1] = indent + key + " " + value
	} else {
		pos := strings.Index(line, "=")
		if format == "ini" && pos < 0 {
			pos = strings.Index(line, ":")
		}
		if format == "git" && pos < 0 {
			indent := line[:len(line)-len(strings.TrimLeft(line, " \t"))]
			key := strings.TrimSpace(line)
			lines[setting.Line-1] = indent + key + "=" + value
			return []byte(strings.Join(lines, "\n")), nil
		}
		if pos < 0 {
			return nil, errors.New("setting is not a key/value line")
		}
		prefix := strings.TrimRight(line[:pos], " \t")
		space := " "
		if pos+1 < len(line) && line[pos+1] != ' ' {
			space = ""
		}
		lines[setting.Line-1] = prefix + " =" + space + value
		if format == "properties" || format == "ini" || format == "git" {
			lines[setting.Line-1] = prefix + "=" + value
		}
	}
	return []byte(strings.Join(lines, "\n")), nil
}

func Validate(format, path string, content []byte) error {
	if bytes.IndexByte(content, 0) >= 0 {
		return errors.New("NUL byte is not valid configuration text")
	}
	switch format {
	case "toml":
		var value any
		return toml.Unmarshal(content, &value)
	case "jsonc":
		if !json.Valid(jsonc.ToJSON(content)) {
			return errors.New("invalid JSON with comments")
		}
	case "bash", "zsh", "fish":
		binary := format
		if _, err := exec.LookPath(binary); err != nil {
			return fmt.Errorf("%s is not installed; syntax check unavailable", binary)
		}
		args := []string{"-n", path}
		cmd := exec.Command(binary, args...)
		if output, err := cmd.CombinedOutput(); err != nil {
			return fmt.Errorf("%s syntax check: %s", binary, strings.TrimSpace(string(output)))
		}
	}
	return nil
}
