package config

import (
	"fmt"
	"os"

	"github.com/BurntSushi/toml"
)

type Link struct {
	Source string `toml:"source"`
	Target string `toml:"target"`
}

type Config struct {
	Link []Link `toml:"link"`
}

func Load(path string) (*Config, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("failed to read config: %s: %w", path, err)
	}

	var cfg Config
	if _, err := toml.Decode(string(content), &cfg); err != nil {
		return nil, fmt.Errorf("failed to parse config: %s: %w", path, err)
	}

	return &cfg, nil
}
