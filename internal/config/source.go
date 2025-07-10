package config

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"gopkg.in/yaml.v3"
)

// SourceCategory represents the category of data being collected
type SourceCategory string

const (
	Environmental  SourceCategory = "environmental"
	Health         SourceCategory = "health"
	Infrastructure SourceCategory = "infrastructure"
	Economic       SourceCategory = "economic"
	Social         SourceCategory = "social"
)

// ValidateCategory checks if the category is valid
func ValidateCategory(category string) bool {
	switch SourceCategory(category) {
	case Environmental, Health, Infrastructure, Economic, Social:
		return true
	default:
		return false
	}
}

type SourceConfig struct {
	Name      string         `yaml:"name"`
	Type      string         `yaml:"type"`
	Category  string         `yaml:"category"`
	Frequency string         `yaml:"frequency"`
	Config    map[string]any `yaml:"config"`
}

func (s *SourceConfig) GetFrequency() (time.Duration, error) {
	return time.ParseDuration(s.Frequency)
}

func (s *SourceConfig) GetCategory() SourceCategory {
	return SourceCategory(s.Category)
}

func (s *SourceConfig) Validate() error {
	if s.Name == "" {
		return fmt.Errorf("source name cannot be empty")
	}
	if s.Type == "" {
		return fmt.Errorf("source type cannot be empty")
	}
	if s.Category == "" {
		return fmt.Errorf("source category cannot be empty")
	}
	if !ValidateCategory(s.Category) {
		return fmt.Errorf("invalid category '%s'. Valid categories are: environmental, health, infrastructure, economic, social", s.Category)
	}
	if s.Frequency == "" {
		return fmt.Errorf("source frequency cannot be empty")
	}
	if _, err := s.GetFrequency(); err != nil {
		return fmt.Errorf("invalid frequency format '%s': %w", s.Frequency, err)
	}
	return nil
}

func LoadSourceConfigs(dir string) ([]SourceConfig, error) {
	var configs []SourceConfig

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if filepath.Ext(path) == ".yaml" || filepath.Ext(path) == ".yml" {
			data, err := os.ReadFile(path)
			if err != nil {
				return fmt.Errorf("failed to read %s: %w", path, err)
			}

			var config SourceConfig
			if err := yaml.Unmarshal(data, &config); err != nil {
				return fmt.Errorf("failed to parse %s: %w", path, err)
			}

			// Validate the configuration
			if err := config.Validate(); err != nil {
				return fmt.Errorf("invalid configuration in %s: %w", path, err)
			}

			configs = append(configs, config)
		}

		return nil
	})

	return configs, err
}
