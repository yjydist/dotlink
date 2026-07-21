package config

import (
	"errors"
	"fmt"
	"path/filepath"

	"github.com/BurntSushi/toml"
)

var ErrConfigParse = errors.New("config parse failure")

type Link struct {
	Source string `toml:"source"`
	Target string `toml:"target"`
}

type Config struct {
	BaseDir string
	Link    []Link `toml:"link"`
}

func Load(path string) (*Config, error) {
	var cfg Config
	if _, err := toml.DecodeFile(path, &cfg); err != nil {
		return nil, fmt.Errorf("%w: %s: %v", ErrConfigParse, path, err)
	}

	for i, l := range cfg.Link {
		if l.Source == "" {
			return nil, fmt.Errorf("%w: link[%d] has empty source", ErrConfigParse, i)
		}
		if l.Target == "" {
			return nil, fmt.Errorf("%w: link[%d] has empty target", ErrConfigParse, i)
		}
	}

	cfg.BaseDir = filepath.Dir(path)
	if !filepath.IsAbs(cfg.BaseDir) {
		abs, err := filepath.Abs(cfg.BaseDir)
		if err != nil {
			return nil, fmt.Errorf("load config %s: %w", path, err)
		}
		cfg.BaseDir = abs
	}

	return &cfg, nil
}
