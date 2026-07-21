package command

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/yjydist/dotlink/internal/config"
	"github.com/yjydist/dotlink/internal/util"
)

type RemoveResult struct {
	Source string
	Target string
	Action string
}

func Remove(cfg *config.Config, dryRun bool) ([]RemoveResult, error) {
	var results []RemoveResult

	for _, l := range cfg.Link {
		source, target, err := util.ResolveLink(cfg.BaseDir, l.Source, l.Target)
		if err != nil {
			return results, fmt.Errorf("resolve link: %w", err)
		}

		tgtInfo, err := os.Lstat(target)
		if errors.Is(err, os.ErrNotExist) {
			results = append(results, RemoveResult{source, target, "not found, skipped"})
			continue
		}
		if err != nil {
			return results, err
		}

		if tgtInfo.Mode()&os.ModeSymlink == 0 {
			results = append(results, RemoveResult{source, target, "not a symlink, skipped"})
			continue
		}

		dest, err := os.Readlink(target)
		if err != nil {
			return results, err
		}
		if filepath.Clean(dest) != source {
			results = append(results, RemoveResult{source, target, "symlink to elsewhere, skipped"})
			continue
		}

		if dryRun {
			results = append(results, RemoveResult{source, target, "would remove"})
			continue
		}

		if err := os.Remove(target); err != nil {
			return results, err
		}
		results = append(results, RemoveResult{source, target, "removed"})
	}

	return results, nil
}
