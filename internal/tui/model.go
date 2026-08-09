package tui

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
	"unicode"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/gutskugou/config-editor/internal/adapters"
	"github.com/gutskugou/config-editor/internal/core"
	"github.com/gutskugou/config-editor/internal/domain"
	"github.com/gutskugou/config-editor/internal/i18n"
	"github.com/mattn/go-runewidth"
	"github.com/mattn/go-shellwords"
)

type focus int

const (
	appsPane focus = iota
	settingsPane
)

type prompt int

const (
	noPrompt prompt = iota
	searchPrompt
	valuePrompt
	confirmPrompt
)

type Model struct {
	apps                        []domain.Application
	manager                     core.Manager
	lang                        i18n.Catalog
	appIndex, settingIndex      int
	focus                       focus
	prompt                      prompt
	input, filter, status, diff string
	pending                     *core.Change
	diffOffset                  int
	width, height               int
}

var (
	titleStyle    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("205"))
	selectedStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("42"))
	dimStyle      = lipgloss.NewStyle().Foreground(lipgloss.Color("245"))
	errorStyle    = lipgloss.NewStyle().Foreground(lipgloss.Color("196"))
)

func New(apps []domain.Application, manager core.Manager, lang i18n.Catalog) Model {
	return Model{apps: apps, manager: manager, lang: lang}
}

func (m Model) Init() tea.Cmd { return nil }

func (m Model) Update(message tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := message.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
	case editorDone:
		if msg.err != nil {
			_ = m.manager.Discard(msg.change)
			m.status = "! " + msg.err.Error()
			return m, nil
		}
		after, err := os.ReadFile(msg.change.StagePath)
		if err != nil {
			_ = m.manager.Discard(msg.change)
			m.status = "! " + err.Error()
			return m, nil
		}
		if string(after) == string(msg.change.Before) {
			_ = m.manager.Discard(msg.change)
			m.status = m.lang.Text("No changes", "没有更改")
			return m, nil
		}
		m.pending, m.diff, m.prompt, m.diffOffset = msg.change, core.SimpleDiff(msg.change.Before, after), confirmPrompt, 0
	case tea.KeyPressMsg:
		key := msg.String()
		if m.prompt != noPrompt {
			return m.updatePrompt(msg)
		}
		switch key {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "up", "k":
			m.move(-1)
		case "down", "j":
			m.move(1)
		case "left", "h", "esc":
			m.focus = appsPane
		case "right", "l", "enter":
			if app, ok := m.currentApp(); ok && settingCount(app) > 0 {
				m.focus = settingsPane
				m.settingIndex = 0
			} else {
				m.status = m.lang.Text("No structured settings; press e to edit a staged copy", "没有结构化设置；按 e 编辑暂存副本")
			}
		case "/":
			m.prompt, m.input = searchPrompt, m.filter
		case "s":
			return m.startStructured()
		case "e":
			return m.startEditor()
		case "r":
			return m.startRestore()
		}
	}
	return m, nil
}

func (m Model) updatePrompt(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	key := msg.String()
	if m.prompt == confirmPrompt {
		switch strings.ToLower(key) {
		case "up", "k":
			m.diffOffset = clamp(m.diffOffset-1, 0, m.maxDiffOffset())
			return m, nil
		case "down", "j":
			m.diffOffset = clamp(m.diffOffset+1, 0, m.maxDiffOffset())
			return m, nil
		case "pgup":
			m.diffOffset = clamp(m.diffOffset-m.diffPageSize(), 0, m.maxDiffOffset())
			return m, nil
		case "pgdown":
			m.diffOffset = clamp(m.diffOffset+m.diffPageSize(), 0, m.maxDiffOffset())
			return m, nil
		case "y":
			result, err := m.manager.Apply(m.pending)
			m.pending = nil
			m.prompt = noPrompt
			m.diff = ""
			m.diffOffset = 0
			if err != nil {
				m.status = "! " + err.Error()
			} else if result.Warning != nil {
				m.status = "! " + result.Warning.Error()
				m.refresh()
			} else {
				m.status = m.lang.Text("Applied safely; snapshot created", "已安全应用并创建快照")
				m.refresh()
			}
			return m, nil
		case "n", "esc":
			if m.pending != nil {
				_ = m.manager.Discard(m.pending)
			}
			m.pending = nil
			m.prompt = noPrompt
			m.diff = ""
			m.diffOffset = 0
			m.status = m.lang.Text("Discarded", "已放弃")
		}
		return m, nil
	}
	switch key {
	case "esc":
		m.prompt = noPrompt
		m.input = ""
		return m, nil
	case "backspace":
		runes := []rune(m.input)
		if len(runes) > 0 {
			m.input = string(runes[:len(runes)-1])
		}
		return m, nil
	case "enter":
		if m.prompt == searchPrompt {
			m.filter = m.input
			m.appIndex = 0
			m.prompt = noPrompt
			return m, nil
		}
		return m.finishStructured()
	}
	if text := msg.Text; text != "" && printable(text) {
		m.input += text
	}
	return m, nil
}

func (m Model) startStructured() (tea.Model, tea.Cmd) {
	_, source, setting, ok := m.selection()
	if !ok || source == nil || setting == nil {
		m.status = m.lang.Text("Select an editable setting", "请选择可编辑的设置")
		return m, nil
	}
	if !setting.Editable {
		m.status = m.lang.Text("Sensitive values are redacted and not edited inline", "敏感值已隐藏，不能行内编辑")
		return m, nil
	}
	m.prompt, m.input = valuePrompt, setting.Value
	return m, nil
}

func (m Model) finishStructured() (tea.Model, tea.Cmd) {
	_, source, setting, ok := m.selection()
	if !ok || source == nil || setting == nil {
		m.prompt = noPrompt
		return m, nil
	}
	change, err := m.manager.Prepare(source.Path, source.Format)
	if err != nil {
		m.prompt = noPrompt
		m.status = "! " + err.Error()
		return m, nil
	}
	content, err := adapters.ReplaceSetting(source.Format, change.Before, *setting, m.input)
	if err == nil {
		err = os.WriteFile(change.StagePath, content, change.Mode)
	}
	if err != nil {
		_ = m.manager.Discard(change)
		m.prompt = noPrompt
		m.status = "! " + err.Error()
		return m, nil
	}
	m.pending, m.diff, m.prompt, m.input, m.diffOffset = change, core.SimpleDiff(change.Before, content), confirmPrompt, "", 0
	return m, nil
}

func (m Model) startEditor() (tea.Model, tea.Cmd) {
	_, source, _, ok := m.selection()
	if !ok || source == nil || !source.Exists {
		m.status = m.lang.Text("Select an existing configuration file", "请选择已有的配置文件")
		return m, nil
	}
	change, err := m.manager.Prepare(source.Path, source.Format)
	if err != nil {
		m.status = "! " + err.Error()
		return m, nil
	}
	editor := os.Getenv("VISUAL")
	if editor == "" {
		editor = os.Getenv("EDITOR")
	}
	if editor == "" {
		editor = "vi"
	}
	parts, err := shellwords.Parse(editor)
	if err != nil || len(parts) == 0 {
		_ = m.manager.Discard(change)
		if err == nil {
			err = fmt.Errorf("editor command is empty")
		}
		m.status = "! " + err.Error()
		return m, nil
	}
	cmd := exec.Command(parts[0], append(parts[1:], change.StagePath)...)
	return m, tea.ExecProcess(cmd, func(err error) tea.Msg { return editorDone{change, err} })
}

type editorDone struct {
	change *core.Change
	err    error
}

func (m Model) startRestore() (tea.Model, tea.Cmd) {
	_, source, _, ok := m.selection()
	if !ok || source == nil || !source.Exists {
		m.status = m.lang.Text("Select an existing configuration file", "请选择已有的配置文件")
		return m, nil
	}
	path := source.Path
	if source.Resolved != "" {
		path = source.Resolved
	}
	change, err := m.manager.PrepareRestore(path, source.Format)
	if err != nil {
		m.status = "! " + err.Error()
		return m, nil
	}
	after, err := os.ReadFile(change.StagePath)
	if err != nil {
		_ = m.manager.Discard(change)
		m.status = "! " + err.Error()
		return m, nil
	}
	m.pending, m.diff, m.prompt, m.diffOffset = change, core.SimpleDiff(change.Before, after), confirmPrompt, 0
	return m, nil
}

func (m Model) View() tea.View {
	view := tea.NewView(m.render())
	view.AltScreen = true
	return view
}

func (m Model) render() string {
	if m.prompt == confirmPrompt && m.diff != "" {
		return m.renderDiff()
	}
	width, height := m.dimensions()
	lines := []string{titleStyle.Render(truncate("Config Editor  "+m.lang.Text("safe configuration workspace", "安全配置工作台"), width)), ""}
	apps := m.filtered()
	footer := m.normalFooter(width)
	available := height - len(lines) - len(footer) - 3
	appRows := minInt(len(apps), maxInt(3, available/2))
	appStart, appEnd := visibleRange(len(apps), m.appIndex, appRows)
	heading := m.lang.Text("Applications", "应用")
	if len(apps) > appRows {
		heading += fmt.Sprintf("  %d-%d/%d", appStart+1, appEnd, len(apps))
	}
	lines = append(lines, truncate(heading, width))
	for i := appStart; i < appEnd; i++ {
		app := apps[i]
		marker := "  "
		if i == m.appIndex && m.focus == appsPane {
			marker = "> "
		}
		state := "·"
		if app.Configured() {
			state = "●"
		} else if app.Installed {
			state = "○"
		}
		name := app.Name
		if m.lang.Chinese {
			name = app.NameZH
		}
		line := truncate(fmt.Sprintf("%s%s %s", marker, state, name), width)
		if i == m.appIndex {
			line = selectedStyle.Render(line)
		}
		lines = append(lines, line)
	}
	if app, ok := m.currentApp(); ok {
		description := app.Description
		name := app.Name
		if m.lang.Chinese {
			description, name = app.DescriptionZH, app.NameZH
		}
		lines = append(lines, "", titleStyle.Render(truncate(name+" — "+description, width)))
		details, selected := m.detailLines(app, width)
		detailRows := maxInt(1, height-len(lines)-len(footer))
		start, end := visibleRange(len(details), selected, detailRows)
		for i := start; i < end; i++ {
			lines = append(lines, details[i])
		}
	}
	lines = append(lines, footer...)
	return strings.Join(lines, "\n")
}

func (m Model) detailLines(app domain.Application, width int) ([]string, int) {
	var lines []string
	selected, row := 0, 0
	for _, source := range app.Sources {
		flag := "missing"
		if source.Exists {
			flag = "file"
		}
		lines = append(lines, dimStyle.Render(truncate(fmt.Sprintf("  [%s] %s", flag, source.Path), width)))
		if source.Diagnostic != "" {
			lines = append(lines, errorStyle.Render(truncate("  ! "+source.Diagnostic, width)))
		}
		for _, setting := range source.Settings {
			marker := "    "
			isSelected := m.focus == settingsPane && row == m.settingIndex
			if isSelected {
				marker, selected = "  > ", len(lines)
			}
			value := truncate(setting.Value, maxInt(8, width-34))
			line := truncate(fmt.Sprintf("%s%-28s %s", marker, setting.Key, value), width)
			if isSelected {
				line = selectedStyle.Render(line)
			}
			lines = append(lines, line)
			row++
		}
	}
	return lines, selected
}

func (m Model) normalFooter(width int) []string {
	var lines []string
	if m.prompt == searchPrompt {
		lines = append(lines, truncate("/ "+m.input+"_", width))
	}
	if m.prompt == valuePrompt {
		lines = append(lines, truncate(m.lang.Text("New value: ", "新值：")+m.input+"_", width))
	}
	if m.status != "" {
		style := dimStyle
		if strings.HasPrefix(m.status, "!") {
			style = errorStyle
		}
		lines = append(lines, style.Render(truncate(m.status, width)))
	}
	lines = append(lines, "", dimStyle.Render(truncate(m.lang.Text("↑↓/jk move  → settings  s set  e edit  r restore  / search  q quit", "↑↓/jk 移动  → 设置  s 修改  e 编辑  r 恢复  / 搜索  q 退出"), width)))
	return lines
}

func (m Model) renderDiff() string {
	width, _ := m.dimensions()
	all := m.diffLines()
	pageSize := m.diffPageSize()
	offset := clamp(m.diffOffset, 0, maxInt(0, len(all)-pageSize))
	end := minInt(len(all), offset+pageSize)
	startDisplay := 0
	if len(all) > 0 {
		startDisplay = offset + 1
	}
	lines := []string{
		titleStyle.Render(truncate(m.lang.Text("Review proposed change", "审阅待应用更改"), width)),
		dimStyle.Render(truncate(fmt.Sprintf("lines %d-%d/%d", startDisplay, end, len(all)), width)),
	}
	for _, line := range all[offset:end] {
		lines = append(lines, truncate(line, width))
	}
	lines = append(lines, m.lang.Text("Apply this change? [y/N]", "应用此更改？[y/N]"), dimStyle.Render(truncate(m.lang.Text("↑↓/jk scroll  PgUp/PgDn page", "↑↓/jk 滚动  PgUp/PgDn 翻页"), width)))
	return strings.Join(lines, "\n")
}

func (m Model) diffLines() []string {
	return strings.Split(strings.TrimSuffix(m.diff, "\n"), "\n")
}

func (m Model) dimensions() (int, int) {
	width, height := m.width, m.height
	if width < 20 {
		width = 80
	}
	if height < 10 {
		height = 24
	}
	return width, height
}

func (m Model) diffPageSize() int {
	_, height := m.dimensions()
	return maxInt(1, height-4)
}

func (m Model) maxDiffOffset() int {
	return maxInt(0, len(m.diffLines())-m.diffPageSize())
}

func visibleRange(total, selected, size int) (int, int) {
	if total <= 0 || size <= 0 {
		return 0, 0
	}
	if size >= total {
		return 0, total
	}
	selected = clamp(selected, 0, total-1)
	start := clamp(selected-size/2, 0, total-size)
	return start, start + size
}

func truncate(value string, width int) string {
	if width <= 0 {
		return ""
	}
	return runewidth.Truncate(value, width, "…")
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func settingCount(app domain.Application) int {
	count := 0
	for _, source := range app.Sources {
		count += len(source.Settings)
	}
	return count
}

func (m Model) filtered() []domain.Application {
	if m.filter == "" {
		return m.apps
	}
	var out []domain.Application
	needle := strings.ToLower(m.filter)
	for _, app := range m.apps {
		if strings.Contains(strings.ToLower(app.Name+" "+app.NameZH+" "+app.ID), needle) {
			out = append(out, app)
		}
	}
	return out
}

func (m Model) currentApp() (domain.Application, bool) {
	apps := m.filtered()
	if len(apps) == 0 {
		return domain.Application{}, false
	}
	index := m.appIndex
	if index >= len(apps) {
		index = len(apps) - 1
	}
	return apps[index], true
}

func (m *Model) move(delta int) {
	if m.focus == appsPane {
		count := len(m.filtered())
		if count > 0 {
			m.appIndex = clamp(m.appIndex+delta, 0, count-1)
			m.settingIndex = 0
		}
		return
	}
	app, ok := m.currentApp()
	if !ok {
		return
	}
	count := 0
	for _, source := range app.Sources {
		count += len(source.Settings)
	}
	if count > 0 {
		m.settingIndex = clamp(m.settingIndex+delta, 0, count-1)
	}
}

func printable(value string) bool {
	for _, r := range value {
		if !unicode.IsPrint(r) {
			return false
		}
	}
	return true
}

func (m Model) selection() (domain.Application, *domain.Source, *domain.Setting, bool) {
	app, ok := m.currentApp()
	if !ok {
		return app, nil, nil, false
	}
	row := 0
	for i := range app.Sources {
		for j := range app.Sources[i].Settings {
			if row == m.settingIndex {
				return app, &app.Sources[i], &app.Sources[i].Settings[j], true
			}
			row++
		}
	}
	for i := range app.Sources {
		if app.Sources[i].Exists {
			return app, &app.Sources[i], nil, true
		}
	}
	return app, nil, nil, false
}

func (m *Model) refresh() {
	for i := range m.apps {
		structured := false
		for _, capability := range m.apps[i].Capabilities {
			if capability == domain.CapabilityStructured {
				structured = true
				break
			}
		}
		if !structured {
			continue
		}
		for j := range m.apps[i].Sources {
			source := &m.apps[i].Sources[j]
			if !source.Exists {
				continue
			}
			if content, err := os.ReadFile(source.Path); err == nil {
				source.Settings = adapters.ParseSettings(source.Format, content)
			}
		}
	}
}

func clamp(value, min, max int) int {
	if value < min {
		return min
	}
	if value > max {
		return max
	}
	return value
}
