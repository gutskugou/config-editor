package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"runtime/debug"

	tea "charm.land/bubbletea/v2"
	"github.com/gutskugou/config-editor/internal/adapters"
	"github.com/gutskugou/config-editor/internal/core"
	"github.com/gutskugou/config-editor/internal/i18n"
	"github.com/gutskugou/config-editor/internal/paths"
	"github.com/gutskugou/config-editor/internal/tui"
)

var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "config-editor:", err)
		os.Exit(1)
	}
}

func run() error {
	if len(os.Args) == 2 && (os.Args[1] == "version" || os.Args[1] == "--version") {
		fmt.Println(versionString())
		return nil
	}
	p, err := paths.Resolve()
	if err != nil {
		return err
	}
	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "scan":
			if len(os.Args) != 3 || os.Args[2] != "--json" {
				return fmt.Errorf("usage: config-editor scan --json")
			}
			apps, err := adapters.Scan(p)
			if err != nil {
				return err
			}
			encoder := json.NewEncoder(os.Stdout)
			encoder.SetIndent("", "  ")
			return encoder.Encode(apps)
		case "doctor":
			fmt.Printf("config-editor doctor\nOS: %s/%s\nHOME: %s\nXDG_CONFIG_HOME: %s\nXDG_STATE_HOME: %s\nAdapters: %d\n", runtime.GOOS, runtime.GOARCH, p.Home, p.Config, p.State, adapters.Builtins())
			for _, command := range []string{"git", "ssh", "bash", "zsh", "fish", "tmux", "vim", "nvim", "code", "starship", "npm", "pip"} {
				_, err := exec.LookPath(command)
				state := "missing"
				if err == nil {
					state = "ok"
				}
				fmt.Printf("%-10s %s\n", command, state)
			}
			return nil
		default:
			return fmt.Errorf("unknown command %q; use scan --json, doctor or version", os.Args[1])
		}
	}
	apps, err := adapters.Scan(p)
	if err != nil {
		return err
	}
	manager := core.Manager{Home: p.Home, ConfigRoot: p.Config, StateRoot: p.State}
	_, err = tea.NewProgram(tui.New(apps, manager, i18n.Detect())).Run()
	return err
}

func versionString() string {
	reported := version
	if reported == "dev" {
		if info, ok := debug.ReadBuildInfo(); ok && info.Main.Version != "" && info.Main.Version != "(devel)" {
			reported = info.Main.Version
		}
	}
	return fmt.Sprintf("config-editor %s (commit %s, built %s)", reported, commit, date)
}
