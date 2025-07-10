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

func (h *HTTPFetcher) Fetch(config map[string]interface{}) ([]*proto.DataPoint, error) {
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
	return h.extractDataPoints(string(jsonBytes), config)
}

func (h *HTTPFetcher) extractDataPoints(jsonData string, config map[string]interface{}) ([]*proto.DataPoint, error) {
	var points []*proto.DataPoint

	// Get the configuration
	source := h.getStringConfig(config, "source", "unknown")
	category := h.getStringConfig(config, "category", "environmental")

	// Check if we have multiple data points
	if dataPoints, ok := config["data_points"].([]interface{}); ok {
		// Handle multiple data points configuration
		for _, dp := range dataPoints {
			if dataPoint, ok := dp.(map[string]interface{}); ok {
				point, err := h.extractSingleDataPoint(jsonData, dataPoint, source, category)
				if err != nil {
					return nil, fmt.Errorf("failed to extract data point: %w", err)
				}
				if point != nil {
					points = append(points, point)
				}
			}
		}
	} else {
		// Handle single data point configuration (backward compatibility)
		point, err := h.extractSingleDataPoint(jsonData, config, source, category)
		if err != nil {
			return nil, fmt.Errorf("failed to extract data point: %w", err)
		}
		if point != nil {
			points = append(points, point)
		}
	}

	return points, nil
}

func (h *HTTPFetcher) extractSingleDataPoint(jsonData string, config map[string]interface{}, source, category string) (*proto.DataPoint, error) {
	// Get the response path
	responsePath := h.getStringConfig(config, "response_path", "")
	if responsePath == "" {
		return nil, fmt.Errorf("response_path is required")
	}

	// Extract value
	result := gjson.Get(jsonData, responsePath)
	if !result.Exists() {
		return nil, fmt.Errorf("response_path %s not found in JSON", responsePath)
	}

	value := result.Float()

	// Get variable and units (can be overridden per data point)
	variable := h.getStringConfig(config, "variable", "unknown")
	units := h.getStringConfig(config, "units", "unknown")

	// Extract coordinates
	var lat, lon float64
	var err error

	if coords, ok := config["coordinates"].(map[string]interface{}); ok {
		lat, lon, err = h.extractCoordinates(jsonData, coords)
		if err != nil {
			return nil, fmt.Errorf("failed to extract coordinates: %w", err)
		}
	} else {
		return nil, fmt.Errorf("coordinates are required")
	}

	// Generate unique ID
	stationId := h.getStringConfig(config, "station_id", "")
	uuid := h.generateUUID(source, variable, stationId)

	point := &proto.DataPoint{
		Source:     source,
		EpochMs:    time.Now().UnixMilli(),
		Value:      value,
		Lat:        lat,
		Lon:        lon,
		Variable:   variable,
		Units:      units,
		Resolution: h.getStringConfig(config, "resolution", "point"),
		Uuid:       uuid,
		Category:   category,
	}

	return point, nil
}

func (h *HTTPFetcher) extractCoordinates(jsonData string, coords map[string]interface{}) (float64, float64, error) {
	var lat, lon float64

	// Try to get latitude
	if latPath, ok := coords["lat_path"].(string); ok {
		latResult := gjson.Get(jsonData, latPath)
		if !latResult.Exists() {
			return 0, 0, fmt.Errorf("lat_path %s not found in JSON", latPath)
		}
		lat = latResult.Float()
	} else if latVal, ok := coords["lat"].(float64); ok {
		lat = latVal
	} else {
		return 0, 0, fmt.Errorf("latitude must be specified via lat_path or lat")
	}

	// Try to get longitude
	if lonPath, ok := coords["lon_path"].(string); ok {
		lonResult := gjson.Get(jsonData, lonPath)
		if !lonResult.Exists() {
			return 0, 0, fmt.Errorf("lon_path %s not found in JSON", lonPath)
		}
		lon = lonResult.Float()
	} else if lonVal, ok := coords["lon"].(float64); ok {
		lon = lonVal
	} else {
		return 0, 0, fmt.Errorf("longitude must be specified via lon_path or lon")
	}

	// Validate coordinates
	if lat < -90.0 || lat > 90.0 {
		return 0, 0, fmt.Errorf("invalid latitude: %f", lat)
	}
	if lon < -180.0 || lon > 180.0 {
		return 0, 0, fmt.Errorf("invalid longitude: %f", lon)
	}

	return lat, lon, nil
}

func (h *HTTPFetcher) generateUUID(source, variable, stationId string) string {
	timestamp := time.Now().UnixNano()
	if stationId != "" {
		return fmt.Sprintf("%s_%s_%s_%d", source, variable, stationId, timestamp)
	}
	return fmt.Sprintf("%s_%s_%d", source, variable, timestamp)
}

func (h *HTTPFetcher) getStringConfig(config map[string]interface{}, key, defaultValue string) string {
	if val, ok := config[key].(string); ok {
		return val
	}
	return defaultValue
}
