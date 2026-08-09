package i18n

import (
	"os"
	"strings"
)

type Catalog struct{ Chinese bool }

func Detect() Catalog {
	lang := strings.ToLower(os.Getenv("LANG"))
	return Catalog{Chinese: strings.HasPrefix(lang, "zh")}
}

func (c Catalog) Text(en, zh string) string {
	if c.Chinese {
		return zh
	}
	return en
}
