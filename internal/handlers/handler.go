package handlers

import (
	"emma/gen/go/proto"
)

type Handler interface {
	Fetch(config map[string]interface{}) ([]*proto.DataPoint, error)
	Validate(config map[string]interface{}) error
}
