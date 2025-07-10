use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::config::ProcessingConfig;
use crate::geo::H3Geocoder;
use crate::proto::DataPoint;

pub struct DataProcessor {
    config: ProcessingConfig,
    geocoder: H3Geocoder,
}

#[derive(Debug, Clone)]
pub struct ProcessedPoint {
    pub data_point: DataPoint,
    pub enriched_data: EnrichedData,
}

#[derive(Debug, Clone, Default)]
pub struct EnrichedData {
    pub country: Option<String>,
    pub region: Option<String>,
    pub timezone: Option<String>,
    pub nearest_place: Option<String>,
    pub h3_cells: Option<[u64; 9]>, // H3 cell IDs for resolutions 0-8
    pub resolution_used: Option<u8>,
    pub calculated_fields: HashMap<String, f64>,
}

impl DataProcessor {
    pub fn new(config: &ProcessingConfig, geocoder: H3Geocoder) -> Self {
        DataProcessor {
            config: config.clone(),
            geocoder,
        }
    }

    pub async fn process(&self, data_point: DataPoint) -> Result<Vec<ProcessedPoint>> {
        let mut processed_points = Vec::new();

        // Step 1: Validation
        if self.config.enable_validation && !self.validate_point(&data_point)? {
            warn!("⚠️  Data point failed validation: {:?}", data_point);
            return Ok(processed_points); // Return empty vec for invalid data
        }

        // Step 2: Enrichment
        let enriched_data = if self.config.enable_enrichment {
            self.enrich_point(&data_point).await?
        } else {
            EnrichedData::default()
        };

        // Step 3: Aggregation (if enabled)
        if self.config.enable_aggregation {
            let aggregated_points = self.aggregate_point(&data_point, &enriched_data)?;
            for point in aggregated_points {
                processed_points.push(ProcessedPoint {
                    data_point: point,
                    enriched_data: enriched_data.clone(),
                });
            }
        } else {
            processed_points.push(ProcessedPoint {
                data_point,
                enriched_data,
            });
        }

        info!("✅ Processed {} data points", processed_points.len());
        Ok(processed_points)
    }

    fn validate_point(&self, point: &DataPoint) -> Result<bool> {
        debug!(
            "🔍 Validating point: {} ({})",
            point.variable, point.category
        );

        // Check for required fields
        if point.source.is_empty() {
            return Ok(false);
        }

        if point.variable.is_empty() {
            return Ok(false);
        }

        if point.category.is_empty() {
            return Ok(false);
        }

        // Validate based on category and variable type
        match point.category.as_str() {
            "environmental" => self.validate_environmental_point(point),
            "health" => self.validate_health_point(point),
            "infrastructure" => self.validate_infrastructure_point(point),
            "economic" => self.validate_economic_point(point),
            "social" => self.validate_social_point(point),
            _ => {
                warn!("Unknown category: {}", point.category);
                Ok(false)
            }
        }
    }

    fn validate_environmental_point(&self, point: &DataPoint) -> Result<bool> {
        match point.variable.as_str() {
            "temperature" => {
                let rules = &self.config.validation_rules;
                if point.value < rules.temperature_min || point.value > rules.temperature_max {
                    warn!(
                        "Environmental temperature out of range: {:.2}°C",
                        point.value
                    );
                    return Ok(false);
                }
            }
            "humidity" => {
                let rules = &self.config.validation_rules;
                if point.value < rules.humidity_min || point.value > rules.humidity_max {
                    warn!("Environmental humidity out of range: {:.2}%", point.value);
                    return Ok(false);
                }
            }
            "air_quality" | "pm2.5" | "pm10" => {
                // Environmental air quality should be non-negative
                if point.value < 0.0 {
                    warn!(
                        "Environmental air quality cannot be negative: {:.2}",
                        point.value
                    );
                    return Ok(false);
                }
            }
            _ => {
                // For other environmental variables, do basic sanity checks
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        self.validate_coordinates(point)
    }

    fn validate_health_point(&self, point: &DataPoint) -> Result<bool> {
        match point.variable.as_str() {
            "heart_rate" => {
                // Health-related heart rate has different ranges
                if point.value < 30.0 || point.value > 250.0 {
                    warn!("Health heart rate out of range: {:.2} bpm", point.value);
                    return Ok(false);
                }
            }
            "temperature" => {
                // Body temperature has different ranges than environmental
                if point.value < 35.0 || point.value > 42.0 {
                    warn!("Health temperature out of range: {:.2}°C", point.value);
                    return Ok(false);
                }
            }
            _ => {
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        self.validate_coordinates(point)
    }

    fn validate_infrastructure_point(&self, point: &DataPoint) -> Result<bool> {
        match point.variable.as_str() {
            "temperature" => {
                // Infrastructure temperature can have wider ranges (e.g., machinery)
                if point.value < -50.0 || point.value > 200.0 {
                    warn!(
                        "Infrastructure temperature out of range: {:.2}°C",
                        point.value
                    );
                    return Ok(false);
                }
            }
            "pressure" => {
                // Infrastructure pressure should be positive
                if point.value <= 0.0 {
                    warn!(
                        "Infrastructure pressure must be positive: {:.2}",
                        point.value
                    );
                    return Ok(false);
                }
            }
            "flow_rate" => {
                // Flow rate should be non-negative
                if point.value < 0.0 {
                    warn!(
                        "Infrastructure flow rate cannot be negative: {:.2}",
                        point.value
                    );
                    return Ok(false);
                }
            }
            _ => {
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        self.validate_coordinates(point)
    }

    fn validate_economic_point(&self, point: &DataPoint) -> Result<bool> {
        match point.variable.as_str() {
            "price" | "cost" | "revenue" => {
                // Economic values should generally be non-negative
                if point.value < 0.0 {
                    warn!("Economic value cannot be negative: {:.2}", point.value);
                    return Ok(false);
                }
            }
            _ => {
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        self.validate_coordinates(point)
    }

    fn validate_social_point(&self, point: &DataPoint) -> Result<bool> {
        match point.variable.as_str() {
            "population" | "count" => {
                // Social counts should be non-negative integers
                if point.value < 0.0 || point.value.fract() != 0.0 {
                    warn!(
                        "Social count must be non-negative integer: {:.2}",
                        point.value
                    );
                    return Ok(false);
                }
            }
            "percentage" | "rate" => {
                // Social percentages should be between 0 and 100
                if point.value < 0.0 || point.value > 100.0 {
                    warn!("Social percentage out of range: {:.2}%", point.value);
                    return Ok(false);
                }
            }
            _ => {
                if point.value.is_nan() || point.value.is_infinite() {
                    return Ok(false);
                }
            }
        }

        self.validate_coordinates(point)
    }

    fn validate_coordinates(&self, point: &DataPoint) -> Result<bool> {
        // Validate coordinates
        if point.lat < -90.0 || point.lat > 90.0 {
            return Ok(false);
        }
        if point.lon < -180.0 || point.lon > 180.0 {
            return Ok(false);
        }

        Ok(true)
    }

    async fn enrich_point(&self, point: &DataPoint) -> Result<EnrichedData> {
        debug!("🌟 Enriching point at ({:.4}, {:.4})", point.lat, point.lon);

        let mut enriched = EnrichedData::default();
        let mut calculated_fields = HashMap::new();

        // Reverse geocoding using H3Geocoder
        if let Some(location) = self
            .geocoder
            .get_complete_location_info(point.lat, point.lon)
        {
            enriched.country = Some(location.country);
            enriched.region = Some(location.region);
            enriched.timezone = Some(location.timezone);
            enriched.nearest_place = Some(location.nearest_place);
            enriched.h3_cells = Some(location.h3_cells);
            enriched.resolution_used = Some(location.resolution_used);
        }

        // Add calculated fields based on category and variable type
        match point.category.as_str() {
            "environmental" => {
                self.add_environmental_calculations(point, &mut calculated_fields);
            }
            "health" => {
                self.add_health_calculations(point, &mut calculated_fields);
            }
            "infrastructure" => {
                self.add_infrastructure_calculations(point, &mut calculated_fields);
            }
            "economic" => {
                self.add_economic_calculations(point, &mut calculated_fields);
            }
            "social" => {
                self.add_social_calculations(point, &mut calculated_fields);
            }
            _ => {}
        }

        enriched.calculated_fields = calculated_fields;
        Ok(enriched)
    }

    fn add_environmental_calculations(
        &self,
        point: &DataPoint,
        calculated_fields: &mut HashMap<String, f64>,
    ) {
        match point.variable.as_str() {
            "temperature" => {}
            "humidity" => {}
            _ => {}
        }
    }

    fn add_health_calculations(
        &self,
        point: &DataPoint,
        calculated_fields: &mut HashMap<String, f64>,
    ) {
        
    }

    fn add_infrastructure_calculations(
        &self,
        point: &DataPoint,
        calculated_fields: &mut HashMap<String, f64>,
    ) {
    }

    fn add_economic_calculations(
        &self,
        point: &DataPoint,
        calculated_fields: &mut HashMap<String, f64>,
    ) {
    }

    fn add_social_calculations(
        &self,
        point: &DataPoint,
        calculated_fields: &mut HashMap<String, f64>,
    ) {
    }

    fn aggregate_point(
        &self,
        point: &DataPoint,
        _enriched: &EnrichedData,
    ) -> Result<Vec<DataPoint>> {
        // For now, just return the original point
        // In a real implementation, you'd accumulate points and create hourly/daily aggregates
        debug!("📊 Aggregating point (placeholder)");
        Ok(vec![point.clone()])
    }
}
