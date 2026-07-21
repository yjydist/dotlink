package main

import (
	"errors"
	"fmt"
	"os"

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
	RunE:  runStatus,
}

var removeCmd = &cobra.Command{
	Use:   "remove",
	Short: "Remove symlinks created by dotlink",
	RunE:  runRemove,
}

func init() {
	rootCmd.AddCommand(applyCmd, statusCmd, removeCmd)

	rootCmd.PersistentFlags().StringVar(&configPath, "config", "dotlink.toml", "path to config file")

	applyCmd.Flags().BoolVar(&force, "force", false, "overwrite existing targets")
	applyCmd.Flags().BoolVar(&dryRun, "dry-run", false, "print actions without executing")

	removeCmd.Flags().BoolVar(&dryRun, "dry-run", false, "print actions without executing")
}

func runApply(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(configPath)
	if err != nil {
		return err
	}

	results, err := command.Apply(cfg, force, dryRun)
	for _, r := range results {
		fmt.Printf("%s: %s -> %s\n", r.Action, r.Source, r.Target)
	}
	return err
}

func runStatus(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(configPath)
	if err != nil {
		return err
	}

	results, err := command.Status(cfg)
	for _, r := range results {
		fmt.Printf("%-18s %s -> %s\n", r.Status, r.Source, r.Target)
	}
	return err
}

func runRemove(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(configPath)
	if err != nil {
		return err
	}

	results, err := command.Remove(cfg, dryRun)
	for _, r := range results {
		fmt.Printf("%s: %s -> %s\n", r.Action, r.Source, r.Target)
	}
	return err
}

func main() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %s\n", err)

		switch {
		case errors.Is(err, config.ErrConfigParse):
			os.Exit(2)
		case errors.Is(err, command.ErrSourceMissing):
			os.Exit(4)
		case errors.Is(err, command.ErrTargetConflict):
			os.Exit(3)
		default:
			os.Exit(1)
		}
	}
}
