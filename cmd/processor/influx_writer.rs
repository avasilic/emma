use anyhow::{anyhow, Result};
use futures::stream;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use tracing::{debug, error, info};

use crate::config::InfluxDbConfig;
use crate::processor::ProcessedPoint;

pub struct InfluxWriter {
    client: Client,
    bucket: String,
}

impl InfluxWriter {
    pub async fn new(config: &InfluxDbConfig) -> Result<Self> {
        let url = format!("http://{}:{}", config.host, config.port);

        let client = Client::new(&url, &config.org, &config.token);

        // Note: Connection will be tested on first write attempt
        info!("🔗 InfluxDB client configured for {}", url);

        Ok(InfluxWriter {
            client,
            bucket: config.bucket.clone(),
        })
    }

    pub async fn write_points(&self, points: Vec<ProcessedPoint>) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        debug!("📝 Writing {} points to InfluxDB", points.len());

        let mut data_points = Vec::new();

        for processed_point in &points {
            let point = &processed_point.data_point;
            let enriched = &processed_point.enriched_data;

            // Convert epoch milliseconds to timestamp
            let timestamp = point.epoch_ms * 1_000_000; // Convert to nanoseconds

            // Create the main data point
            let mut builder = DataPoint::builder("data_points")
                .tag("source", &point.source)
                .tag("category", &point.category)
                .tag("variable", &point.variable)
                .tag("units", &point.units)
                .tag("country", enriched.country.as_deref().unwrap_or("unknown"))
                .tag("region", enriched.region.as_deref().unwrap_or("unknown"))
                .field("value", point.value)
                .field("lat", point.lat)
                .field("lon", point.lon)
                .timestamp(timestamp);

            // Add H3 cell information if available
            if let Some(h3_cells) = &enriched.h3_cells {
                for (resolution, &cell_id) in h3_cells.iter().enumerate() {
                    builder = builder.field(format!("h3_cell_res_{resolution}"), cell_id as i64);
                }
            }

            // Add other enriched fields
            if let Some(nearest_place) = &enriched.nearest_place {
                builder = builder.tag("nearest_place", nearest_place);
            }

            if let Some(timezone) = &enriched.timezone {
                builder = builder.tag("timezone", timezone);
            }

            if let Some(resolution_used) = enriched.resolution_used {
                builder = builder.field("h3_resolution_used", resolution_used as i64);
            }

            data_points.push(builder.build()?);

            // Write H3 spatial data as a dedicated measurement for efficient spatial queries
            if let Some(h3_cells) = &enriched.h3_cells {
                let mut h3_builder = DataPoint::builder("h3_spatial")
                    .tag("source", &point.source)
                    .tag("category", &point.category)
                    .tag("variable", &point.variable)
                    .tag("country", enriched.country.as_deref().unwrap_or("unknown"))
                    .tag("region", enriched.region.as_deref().unwrap_or("unknown"))
                    .field("lat", point.lat)
                    .field("lon", point.lon)
                    .timestamp(timestamp);

                // Add all H3 cell IDs as fields for efficient spatial queries
                for (resolution, &cell_id) in h3_cells.iter().enumerate() {
                    h3_builder =
                        h3_builder.field(format!("h3_cell_res_{resolution}"), cell_id as i64);
                }

                if let Some(nearest_place) = &enriched.nearest_place {
                    h3_builder = h3_builder.tag("nearest_place", nearest_place);
                }

                if let Some(timezone) = &enriched.timezone {
                    h3_builder = h3_builder.tag("timezone", timezone);
                }

                if let Some(resolution_used) = enriched.resolution_used {
                    h3_builder = h3_builder.field("h3_resolution_used", resolution_used as i64);
                }

                data_points.push(h3_builder.build()?);
            }

            // Write calculated fields as separate measurements
            for (field_name, field_value) in &enriched.calculated_fields {
                let mut calculated_builder = DataPoint::builder("calculated_fields")
                    .tag("source", &point.source)
                    .tag("category", &point.category)
                    .tag("variable", field_name)
                    .tag("original_variable", &point.variable)
                    .tag(
                        "units",
                        self.get_units_for_calculated_field(field_name, &point.category),
                    )
                    .tag("country", enriched.country.as_deref().unwrap_or("unknown"))
                    .tag("region", enriched.region.as_deref().unwrap_or("unknown"))
                    .field("value", *field_value)
                    .field("lat", point.lat)
                    .field("lon", point.lon)
                    .timestamp(timestamp);

                // Add H3 cell information to calculated fields too
                if let Some(h3_cells) = &enriched.h3_cells {
                    for (resolution, &cell_id) in h3_cells.iter().enumerate() {
                        calculated_builder = calculated_builder
                            .field(format!("h3_cell_res_{resolution}"), cell_id as i64);
                    }
                }

                // Add other enriched fields
                if let Some(nearest_place) = &enriched.nearest_place {
                    calculated_builder = calculated_builder.tag("nearest_place", nearest_place);
                }

                if let Some(timezone) = &enriched.timezone {
                    calculated_builder = calculated_builder.tag("timezone", timezone);
                }

                if let Some(resolution_used) = enriched.resolution_used {
                    calculated_builder =
                        calculated_builder.field("h3_resolution_used", resolution_used as i64);
                }

                data_points.push(calculated_builder.build()?);
            }
        }

        // Write all points at once
        match self
            .client
            .write(&self.bucket, stream::iter(data_points))
            .await
        {
            Ok(_) => {
                info!("✅ Successfully wrote {} points to InfluxDB", points.len());
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to write points to InfluxDB: {}", e);
                Err(anyhow!("Failed to write to InfluxDB: {}", e))
            }
        }
    }

    fn get_units_for_calculated_field(&self, field_name: &str, category: &str) -> String {
        match category {
            "environmental" => match field_name {
                "temperature_fahrenheit" => "fahrenheit".to_string(),
                "temperature_kelvin" => "kelvin".to_string(),
                "dew_point" => "celsius".to_string(),
                _ => "unknown".to_string(),
            },
            "health" => match field_name {
                "body_temperature_fahrenheit" => "fahrenheit".to_string(),
                "heart_rate_percentage" => "percentage".to_string(),
                _ => "unknown".to_string(),
            },
            "infrastructure" => match field_name {
                "flow_rate_m3_per_hour" => "m³/h".to_string(),
                "pressure_psi" => "psi".to_string(),
                _ => "unknown".to_string(),
            },
            "economic" => match field_name {
                "price_per_unit" => "currency/unit".to_string(),
                _ => "unknown".to_string(),
            },
            "social" => match field_name {
                "population_density" => "people/km²".to_string(),
                _ => "unknown".to_string(),
            },
            _ => "unknown".to_string(),
        }
    }
}
