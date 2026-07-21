package command

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/yjydist/dotlink/internal/config"
	"github.com/yjydist/dotlink/internal/util"
)

type LinkStatus string

const (
	StatusLinkedCorrect   LinkStatus = "linked-correct"
	StatusLinkedElsewhere LinkStatus = "linked-elsewhere"
	StatusExistsNotLink   LinkStatus = "exists-not-link"
	StatusMissing         LinkStatus = "missing"
	StatusSourceMissing   LinkStatus = "source-missing"
)

type StatusResult struct {
	Source string
	Target string
	Status LinkStatus
}

func Status(cfg *config.Config) ([]StatusResult, error) {
	var results []StatusResult

	for _, l := range cfg.Link {
		source, target, err := util.ResolveLink(cfg.BaseDir, l.Source, l.Target)
		if err != nil {
			return results, fmt.Errorf("resolve link: %w", err)
		}

		if _, err := os.Stat(source); err != nil {
			if errors.Is(err, os.ErrNotExist) {
				results = append(results, StatusResult{source, target, StatusSourceMissing})
				continue
			}
			return results, err
		}

		tgtInfo, err := os.Lstat(target)
		if errors.Is(err, os.ErrNotExist) {
			results = append(results, StatusResult{source, target, StatusMissing})
			continue
		}
		if err != nil {
			return results, err
		}

		if tgtInfo.Mode()&os.ModeSymlink == 0 {
			results = append(results, StatusResult{source, target, StatusExistsNotLink})
			continue
		}

		dest, err := os.Readlink(target)
		if err != nil {
			return results, err
		}
		if filepath.Clean(dest) == source {
			results = append(results, StatusResult{source, target, StatusLinkedCorrect})
		} else {
			results = append(results, StatusResult{source, target, StatusLinkedElsewhere})
		}
	}

	return results, nil
}
