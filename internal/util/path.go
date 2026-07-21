package util

import (
	"os"
	"path/filepath"
	"strings"
)

// ExpandPath expands ~, $VAR, and ${VAR} in path.
func ExpandPath(path string) (string, error) {
	if path == "" {
		return path, nil
	}

	if strings.HasPrefix(path, "~") {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		path = home + path[1:]
	}

	return os.ExpandEnv(path), nil
}

// ResolveLink resolves source and target from config entries into absolute paths.
// Relative sources resolve against baseDir; relative targets resolve against cwd.
func ResolveLink(baseDir, srcRaw, tgRaw string) (source string, target string, err error) {
	source, err = ExpandPath(srcRaw)
	if err != nil {
		return "", "", err
	}
	if !filepath.IsAbs(source) {
		source = filepath.Join(baseDir, source)
	}
	source = filepath.Clean(source)

	target, err = ExpandPath(tgRaw)
	if err != nil {
		return "", "", err
	}
	target, err = filepath.Abs(target)
	if err != nil {
		return "", "", err
	}

	return source, target, nil
}
