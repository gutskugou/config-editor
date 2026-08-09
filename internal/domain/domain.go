package domain

type Capability string

const (
	CapabilityDiscover   Capability = "discover"
	CapabilityStructured Capability = "structured"
	CapabilitySyntax     Capability = "syntax-check"
	CapabilityRaw        Capability = "staged-editor"
)

type Setting struct {
	Key       string `json:"key"`
	Value     string `json:"value"`
	Line      int    `json:"line"`
	Editable  bool   `json:"editable"`
	Sensitive bool   `json:"sensitive"`
}

type Source struct {
	Path       string    `json:"path"`
	Resolved   string    `json:"resolved,omitempty"`
	Exists     bool      `json:"exists"`
	Format     string    `json:"format"`
	Diagnostic string    `json:"diagnostic,omitempty"`
	Settings   []Setting `json:"settings,omitempty"`
}

type Application struct {
	ID            string       `json:"id"`
	Name          string       `json:"name"`
	NameZH        string       `json:"name_zh"`
	Description   string       `json:"description"`
	DescriptionZH string       `json:"description_zh"`
	Command       string       `json:"command,omitempty"`
	Installed     bool         `json:"installed"`
	Capabilities  []Capability `json:"capabilities"`
	Sources       []Source     `json:"sources"`
}

func (a Application) Configured() bool {
	for _, source := range a.Sources {
		if source.Exists {
			return true
		}
	}
	return false
}
