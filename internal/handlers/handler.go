package handlers

import (
	"emma/gen/go/proto/v1"
)

type Handler interface {
	Fetch(config map[string]any) ([]*v1.DataPoint, error)
	Validate(config map[string]any) error
}
