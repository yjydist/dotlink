package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"

	"github.com/yjydist/dotlink/internal/command"
	"github.com/yjydist/dotlink/internal/config"
)

var (
	configPath string
	force      bool
	dryRun     bool
)

var rootCmd = &cobra.Command{
	Use:   "dotlink",
	Short: "A dotfile link manager",
}

var applyCmd = &cobra.Command{
	Use:   "apply",
	Short: "Apply links from the config file",
	RunE:  runApply,
}

var statusCmd = &cobra.Command{
	Use:   "status",
	Short: "Show current status of each link",
	Run:   func(cmd *cobra.Command, args []string) {},
}

var removeCmd = &cobra.Command{
	Use:   "remove",
	Short: "Remove symlinks created by dotlink",
	Run:   func(cmd *cobra.Command, args []string) {},
}

func init() {
	rootCmd.AddCommand(applyCmd, statusCmd, removeCmd)

	applyCmd.Flags().StringVar(&configPath, "config", "dotlink.toml", "path to config file")
	applyCmd.Flags().BoolVar(&force, "force", false, "overwrite existing targets")
	applyCmd.Flags().BoolVar(&dryRun, "dry-run", false, "print actions without executing")

	statusCmd.Flags().StringVar(&configPath, "config", "dotlink.toml", "path to config file")
	removeCmd.Flags().StringVar(&configPath, "config", "dotlink.toml", "path to config file")
}

func runApply(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(configPath)
	if err != nil {
		return fmt.Errorf("load config: %w", err)
	}

	results, err := command.Apply(configPath, cfg, force, dryRun)
	for _, r := range results {
		fmt.Printf("%s: %s -> %s\n", r.Action, r.Source, r.Target)
	}
	if err != nil {
		return err
	}
	return nil
}

func main() {
	if err := rootCmd.Execute(); err != nil {
		msg := err.Error()
		fmt.Fprintf(os.Stderr, "error: %s\n", msg)

		switch {
		case strings.Contains(msg, "load config"):
			os.Exit(2)
		case strings.Contains(msg, "source missing"):
			os.Exit(4)
		case strings.Contains(msg, "target exists") || strings.Contains(msg, "symlink to elsewhere"):
			os.Exit(3)
		default:
			os.Exit(1)
		}
	}
}
