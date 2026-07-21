package command

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/yjydist/dotlink/internal/config"
	"github.com/yjydist/dotlink/internal/util"
)

var (
	ErrSourceMissing  = errors.New("source missing")
	ErrTargetConflict = errors.New("target conflict")
)

type ApplyResult struct {
	Source string
	Target string
	Action string
}

func Apply(cfg *config.Config, force, dryRun bool) ([]ApplyResult, error) {
	var results []ApplyResult

	for _, l := range cfg.Link {
		source, target, err := util.ResolveLink(cfg.BaseDir, l.Source, l.Target)
		if err != nil {
			return results, fmt.Errorf("resolve link: %w", err)
		}

		if _, err := os.Stat(source); err != nil {
			if errors.Is(err, os.ErrNotExist) {
				return results, fmt.Errorf("%w: %s", ErrSourceMissing, source)
			}
			return results, err
		}

		tgtInfo, err := os.Lstat(target)

		if errors.Is(err, os.ErrNotExist) {
			if dryRun {
				results = append(results, ApplyResult{source, target, "would create symlink"})
				continue
			}
			if err := createSymlink(source, target); err != nil {
				return results, err
			}
			results = append(results, ApplyResult{source, target, "created symlink"})
			continue
		}
		if err != nil {
			return results, err
		}

		if tgtInfo.Mode()&os.ModeSymlink == 0 {
			if !force {
				return results, fmt.Errorf("%w: %s exists and is not a symlink", ErrTargetConflict, target)
			}
			if dryRun {
				results = append(results, ApplyResult{source, target, "would replace existing file"})
				continue
			}
			if err := os.RemoveAll(target); err != nil {
				return results, err
			}
		} else {
			dest, err := os.Readlink(target)
			if err != nil {
				return results, err
			}
			if filepath.Clean(dest) == source {
				results = append(results, ApplyResult{source, target, "already linked"})
				continue
			}
			if !force {
				return results, fmt.Errorf("%w: %s is symlink to elsewhere: %s", ErrTargetConflict, target, dest)
			}
			if dryRun {
				results = append(results, ApplyResult{source, target, "would replace symlink"})
				continue
			}
			if err := os.Remove(target); err != nil {
				return results, err
			}
		}

		if err := createSymlink(source, target); err != nil {
			return results, err
		}
		results = append(results, ApplyResult{source, target, "recreated symlink"})
	}

	return results, nil
}

func createSymlink(source, target string) error {
	if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
		return err
	}
	return os.Symlink(source, target)
}
