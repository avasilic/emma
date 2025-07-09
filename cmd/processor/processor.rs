use anyhow::Result;

use tracing::{debug, info, warn};

use crate::config::ProcessingConfig;
use crate::proto::ClimatePoint;

pub struct DataProcessor {
    config: ProcessingConfig,
}

#[derive(Debug, Clone)]
pub struct ProcessedPoint {
    pub climate_point: ClimatePoint,
    pub quality_score: f64,
    pub enriched_data: EnrichedData,
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct EnrichedData {
    pub country: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub elevation: Option<f64>,
    pub calculated_fields: std::collections::HashMap<String, f64>,
}

impl DataProcessor {
    pub fn new(config: &ProcessingConfig) -> Self {
        DataProcessor {
            config: config.clone(),
        }
    }

    pub async fn process(&self, climate_point: ClimatePoint) -> Result<Vec<ProcessedPoint>> {
        let mut processed_points = Vec::new();

        // Step 1: Validation
        if self.config.enable_validation && !self.validate_point(&climate_point)? {
                warn!("⚠️  Data point failed validation: {:?}", climate_point);
                return Ok(processed_points); // Return empty vec for invalid data
            }

        // Step 2: Enrichment
        let enriched_data = if self.config.enable_enrichment {
            self.enrich_point(&climate_point).await?
        } else {
            EnrichedData::default()
        };

        // Step 3: Quality scoring
        let quality_score = self.calculate_quality_score(&climate_point, &enriched_data);

        // Step 4: Aggregation (if enabled)
        if self.config.enable_aggregation {
            let aggregated_points = self.aggregate_point(&climate_point, &enriched_data)?;
            for point in aggregated_points {
                processed_points.push(ProcessedPoint {
                    climate_point: point,
                    quality_score,
                    enriched_data: enriched_data.clone(),
                });
            }
        } else {
            processed_points.push(ProcessedPoint {
                climate_point,
                quality_score,
                enriched_data,
            });
        }

        info!("✅ Processed {} data points", processed_points.len());
        Ok(processed_points)
    }

    fn validate_point(&self, point: &ClimatePoint) -> Result<bool> {
        debug!("🔍 Validating point: {}", point.variable);

        // Check for required fields
        if point.source.is_empty() {
            return Ok(false);
        }

        if point.variable.is_empty() {
            return Ok(false);
        }

        // Validate based on variable type
        match point.variable.as_str() {
            "temperature" => {
                let rules = &self.config.validation_rules;
                if point.value < rules.temperature_min || point.value > rules.temperature_max {
                    warn!("Temperature out of range: {:.2}°C", point.value);
                    return Ok(false);
                }
            }
            "humidity" => {
                let rules = &self.config.validation_rules;
                if point.value < rules.humidity_min || point.value > rules.humidity_max {
                    warn!("Humidity out of range: {:.2}%", point.value);
                    return Ok(false);
                }
            }
            _ => {
                // For other variables, do basic sanity checks
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        // Validate coordinates
        if point.lat < -90.0 || point.lat > 90.0 {
            return Ok(false);
        }
        if point.lon < -180.0 || point.lon > 180.0 {
            return Ok(false);
        }

        Ok(true)
    }

    async fn enrich_point(&self, point: &ClimatePoint) -> Result<EnrichedData> {
        debug!("🌟 Enriching point at ({:.4}, {:.4})", point.lat, point.lon);

        let mut enriched = EnrichedData::default();
        let mut calculated_fields = std::collections::HashMap::new();

        // Reverse geocoding (simplified - in real app you'd use a geocoding API)
        enriched.country = self.get_country_from_coords(point.lat, point.lon);
        enriched.region = self.get_region_from_coords(point.lat, point.lon);
        enriched.timezone = self.get_timezone_from_coords(point.lat, point.lon);

        // Add calculated fields based on variable type
        match point.variable.as_str() {
            "temperature" => {
                // Convert Celsius to Fahrenheit
                let fahrenheit = (point.value * 9.0 / 5.0) + 32.0;
                calculated_fields.insert("temperature_fahrenheit".to_string(), fahrenheit);

                // Convert to Kelvin
                let kelvin = point.value + 273.15;
                calculated_fields.insert("temperature_kelvin".to_string(), kelvin);
            }
            "humidity" => {
                // Calculate dew point (simplified formula)
                let dew_point = point.value - ((100.0 - point.value) / 5.0);
                calculated_fields.insert("dew_point".to_string(), dew_point);
            }
            _ => {}
        }

        enriched.calculated_fields = calculated_fields;
        Ok(enriched)
    }

    fn calculate_quality_score(&self, point: &ClimatePoint, enriched: &EnrichedData) -> f64 {
        let mut score: f64 = 1.0;

        // Reduce score for missing location data
        if point.lat == 0.0 && point.lon == 0.0 {
            score *= 0.7;
        }

        // Reduce score for unknown source
        if point.source == "unknown" {
            score *= 0.8;
        }

        // Increase score for enriched data
        if enriched.country.is_some() {
            score *= 1.1;
        }

        // Clamp between 0 and 1
        score.clamp(0.0, 1.0)
    }

    fn aggregate_point(
        &self,
        point: &ClimatePoint,
        _enriched: &EnrichedData,
    ) -> Result<Vec<ClimatePoint>> {
        // For now, just return the original point
        // In a real implementation, you'd accumulate points and create hourly/daily aggregates
        debug!("📊 Aggregating point (placeholder)");
        Ok(vec![point.clone()])
    }

    // Helper methods for geocoding (simplified)
    fn get_country_from_coords(&self, lat: f64, lon: f64) -> Option<String> {
        // Simplified geocoding - in real app use a proper geocoding service
        if lat > 35.0 && lat < 46.0 && lon > 138.0 && lon < 146.0 {
            Some("Japan".to_string())
        } else if lat > 40.0 && lat < 50.0 && lon > -125.0 && lon < -66.0 {
            Some("United States".to_string())
        } else {
            None
        }
    }

    fn get_region_from_coords(&self, lat: f64, lon: f64) -> Option<String> {
        if lat > 35.0 && lat < 36.0 && lon > 139.0 && lon < 140.0 {
            Some("Tokyo".to_string())
        } else {
            None
        }
    }

    fn get_timezone_from_coords(&self, lat: f64, lon: f64) -> Option<String> {
        if lat > 35.0 && lat < 46.0 && lon > 138.0 && lon < 146.0 {
            Some("Asia/Tokyo".to_string())
        } else {
            None
        }
    }
}


