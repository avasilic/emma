package config

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"gopkg.in/yaml.v3"
)

type SourceConfig struct {
	Name      string                 `yaml:"name"`
	Type      string                 `yaml:"type"`
	Frequency string                 `yaml:"frequency"`
	Config    map[string]interface{} `yaml:"config"`
}

func (s *SourceConfig) GetFrequency() (time.Duration, error) {
	return time.ParseDuration(s.Frequency)
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

			configs = append(configs, config)
		}

		return nil
	})

	return configs, err
}
