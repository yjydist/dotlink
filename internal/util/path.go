package util

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

var envVarRe = regexp.MustCompile(`\$(\w+|\{\w+\})`)

// ExpandPath
// 展开路径, 包括 "~", "$VAR", "${VAR}"
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

	path = envVarRe.ReplaceAllStringFunc(path, func(s string) string {
		name := strings.TrimPrefix(s, "$")
		name = strings.Trim(name, "{}")
		return os.Getenv(name)
	})

	return path, nil

}

// ResolveLink
// 把 Config 中的 source 和 target 解析成绝对路径
func ResolveLink(cfgPath string, srcRaw string, tgRaw string) (source string, target string, err error) {
	cfgDir := filepath.Dir(cfgPath)
	if !filepath.IsAbs(cfgDir) {
		cfgDir, err = filepath.Abs(cfgDir)
		if err != nil {
			return "", "", err
		}
	}

	source, err = ExpandPath(srcRaw)
	if err != nil {
		return "", "", err
	}
	if !filepath.IsAbs(source) {
		source = filepath.Join(cfgDir, source)
	}
	source = filepath.Clean(source)

	target, err = ExpandPath(tgRaw)
	if err != nil {
		return "", "", err
	}
	if !filepath.IsAbs(target) {
		cwd, err := os.Getwd()
		if err != nil {
			return "", "", err
		}
		target = filepath.Join(cwd, target)
	}
	target = filepath.Clean(target)

	return source, target, nil
}
