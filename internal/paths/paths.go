package paths

import (
	"os"
	"path/filepath"
)

type Paths struct {
	Home   string
	Config string
	State  string
	Cache  string
}

func Resolve() (Paths, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return Paths{}, err
	}
	return Paths{
		Home:   home,
		Config: base("XDG_CONFIG_HOME", filepath.Join(home, ".config")),
		State:  base("XDG_STATE_HOME", filepath.Join(home, ".local", "state")),
		Cache:  base("XDG_CACHE_HOME", filepath.Join(home, ".cache")),
	}, nil
}

func base(name, fallback string) string {
	if value := os.Getenv(name); filepath.IsAbs(value) {
		return filepath.Clean(value)
	}
	return fallback
}
