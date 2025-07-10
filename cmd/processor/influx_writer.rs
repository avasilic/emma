use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use influxdb::{Client, WriteQuery};
use tracing::{debug, error, info};

use crate::config::InfluxDbConfig;
use crate::processor::ProcessedPoint;

pub struct InfluxWriter {
    client: Client,
    database: String,
}

impl InfluxWriter {
    pub async fn new(config: &InfluxDbConfig) -> Result<Self> {
        let url = format!("http://{}:{}", config.host, config.port);

        let mut client = Client::new(&url, &config.database);

        // Add authentication if provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client = client.with_auth(username, password);
        }

        // Test the connection
        let ping_result = client.ping().await;
        match ping_result {
            Ok(_) => {
                info!("🔗 Connected to InfluxDB at {}", url);
            }
            Err(e) => {
                error!("❌ Failed to connect to InfluxDB: {}", e);
                return Err(anyhow!("Failed to connect to InfluxDB: {}", e));
            }
        }

        Ok(InfluxWriter {
            client,
            database: config.database.clone(),
        })
    }

    pub async fn write_points(&self, points: Vec<ProcessedPoint>) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }

        debug!("📝 Writing {} points to InfluxDB", points.len());

        let mut write_queries = Vec::new();

        for processed_point in &points {
            let point = &processed_point.data_point;
            let enriched = &processed_point.enriched_data;

            // Convert epoch milliseconds to DateTime
            let timestamp =
                DateTime::from_timestamp_millis(point.epoch_ms).unwrap_or_else(|| Utc::now());

            // Create the main data point
            let write_query = WriteQuery::new(timestamp.into(), "data_points")
                .add_tag("source", point.source.as_str())
                .add_tag("category", point.category.as_str())
                .add_tag("variable", point.variable.as_str())
                .add_tag("units", point.units.as_str())
                .add_tag("country", enriched.country.as_deref().unwrap_or("unknown"))
                .add_tag("region", enriched.region.as_deref().unwrap_or("unknown"))
                .add_field("value", point.value)
                .add_field("lat", point.lat)
                .add_field("lon", point.lon);

            write_queries.push(write_query);

            // Write calculated fields as separate measurements
            for (field_name, field_value) in &enriched.calculated_fields {
                let calculated_query = WriteQuery::new(timestamp.into(), "calculated_fields")
                    .add_tag("source", point.source.as_str())
                    .add_tag("category", point.category.as_str())
                    .add_tag("variable", field_name.as_str())
                    .add_tag("original_variable", point.variable.as_str())
                    .add_tag(
                        "units",
                        self.get_units_for_calculated_field(field_name, &point.category)
                            .as_str(),
                    )
                    .add_tag("country", enriched.country.as_deref().unwrap_or("unknown"))
                    .add_tag("region", enriched.region.as_deref().unwrap_or("unknown"))
                    .add_field("value", *field_value)
                    .add_field("lat", point.lat)
                    .add_field("lon", point.lon);

                write_queries.push(calculated_query);
            }
        }

        // Execute all write queries
        for write_query in write_queries {
            match self.client.query(write_query).await {
                Ok(_) => {
                    debug!("✅ Successfully wrote point to InfluxDB");
                }
                Err(e) => {
                    error!("❌ Failed to write point to InfluxDB: {}", e);
                    return Err(anyhow!("Failed to write to InfluxDB: {}", e));
                }
            }
        }

        info!("✅ Successfully wrote {} points to InfluxDB", points.len());
        Ok(())
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
