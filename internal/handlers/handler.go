package handlers

import (
	"emma/gen/go/proto/v1"
)

type Handler interface {
	Fetch(config map[string]interface{}) ([]*v1.DataPoint, error)
	Validate(config map[string]interface{}) error
}
