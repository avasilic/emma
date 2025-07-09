package handlers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"time"

	"emma/gen/go/proto"
	"github.com/tidwall/gjson"
)

type HTTPFetcher struct {
	client *http.Client
}

func NewHTTPFetcher() *HTTPFetcher {
	return &HTTPFetcher{
		client: &http.Client{
			Timeout: 30 * time.Second,
		},
	}
}

func (h *HTTPFetcher) Validate(config map[string]interface{}) error {
	if _, ok := config["url"]; !ok {
		return fmt.Errorf("url is required for http_fetch")
	}
	return nil
}

func (h *HTTPFetcher) Fetch(config map[string]interface{}) ([]*proto.ClimatePoint, error) {
	url := config["url"].(string)
	method := h.getStringConfig(config, "method", "GET")

	// Build request
	req, err := http.NewRequest(method, url, nil)
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	// Add headers
	if headers, ok := config["headers"].([]interface{}); ok {
		for _, header := range headers {
			if h, ok := header.(map[string]interface{}); ok {
				key := h["key"].(string)
				value := h["value"].(string)

				// Replace environment variables
				if strings.HasPrefix(value, "${") && strings.HasSuffix(value, "}") {
					envVar := strings.TrimPrefix(strings.TrimSuffix(value, "}"), "${")
					value = os.Getenv(envVar)
				}

				req.Header.Set(key, value)
			}
		}
	}

	// Add query parameters
	if params, ok := config["params"].([]interface{}); ok {
		q := req.URL.Query()
		for _, param := range params {
			if p, ok := param.(map[string]interface{}); ok {
				key := p["key"].(string)
				value := p["value"].(string)
				q.Add(key, value)
			}
		}
		req.URL.RawQuery = q.Encode()
	}

	// Make request
	resp, err := h.client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("failed to fetch data: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("HTTP %d: %s", resp.StatusCode, resp.Status)
	}

	// Parse JSON response
	var jsonData interface{}
	if err := json.NewDecoder(resp.Body).Decode(&jsonData); err != nil {
		return nil, fmt.Errorf("failed to parse JSON: %w", err)
	}

	jsonBytes, _ := json.Marshal(jsonData)

	// Extract data using JSONPath
	return h.extractClimatePoints(string(jsonBytes), config)
}

func (h *HTTPFetcher) extractClimatePoints(jsonData string, config map[string]interface{}) ([]*proto.ClimatePoint, error) {
	var points []*proto.ClimatePoint

	// Get the main data path
	responsePath := h.getStringConfig(config, "response_path", "$.main.temp")
	variable := h.getStringConfig(config, "variable", "temperature")
	units := h.getStringConfig(config, "units", "celsius")
	source := h.getStringConfig(config, "source", "unknown")

	// Extract value
	result := gjson.Get(jsonData, responsePath)
	if !result.Exists() {
		return nil, fmt.Errorf("response_path %s not found in JSON", responsePath)
	}

	value := result.Float()

	// Extract coordinates
	var lat, lon float64
	if coords, ok := config["coordinates"].(map[string]interface{}); ok {
		if latPath, ok := coords["lat_path"].(string); ok {
			lat = gjson.Get(jsonData, latPath).Float()
		} else if latVal, ok := coords["lat"].(float64); ok {
			lat = latVal
		}

		if lonPath, ok := coords["lon_path"].(string); ok {
			lon = gjson.Get(jsonData, lonPath).Float()
		} else if lonVal, ok := coords["lon"].(float64); ok {
			lon = lonVal
		}
	}

	point := &proto.ClimatePoint{
		Source:     source,
		EpochMs:    time.Now().UnixMilli(),
		Value:      value,
		Lat:        lat,
		Lon:        lon,
		Variable:   variable,
		Units:      units,
		Resolution: "point",
		Uuid:       fmt.Sprintf("%s_%d", source, time.Now().UnixNano()),
	}

	points = append(points, point)
	return points, nil
}

func (h *HTTPFetcher) getStringConfig(config map[string]interface{}, key, defaultValue string) string {
	if val, ok := config[key].(string); ok {
		return val
	}
	return defaultValue
}
