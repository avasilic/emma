use anyhow::Result;
use config::{Config, ConfigError, Environment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ProcessorConfig {
    pub kafka: KafkaConfig,
    pub processing: ProcessingConfig,
    pub influxdb: InfluxDbConfig,
    pub geocoder: GeocoderConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub group_id: String,
    pub topic: String,
    pub auto_offset_reset: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProcessingConfig {
    pub enable_validation: bool,
    pub enable_enrichment: bool,
    pub enable_aggregation: bool,
    pub batch_size: usize,
    pub validation_rules: ValidationRules,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ValidationRules {
    pub temperature_min: f64,
    pub temperature_max: f64,
    pub humidity_min: f64,
    pub humidity_max: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InfluxDbConfig {
    pub host: String,
    pub port: u16,
    pub org: String,
    pub bucket: String,
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeocoderConfig {
    pub geonames_file_path: String,
}

impl ProcessorConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config = Config::builder()
            // Default values
            .set_default("kafka.bootstrap_servers", "localhost:9092")?
            .set_default("kafka.group_id", "climate-processor")?
            .set_default("kafka.topic", "weather.raw")?
            .set_default("kafka.auto_offset_reset", "earliest")?
            .set_default("processing.enable_validation", true)?
            .set_default("processing.enable_enrichment", true)?
            .set_default("processing.enable_aggregation", true)?
            .set_default("processing.batch_size", 100)?
            .set_default("processing.validation_rules.temperature_min", -100.0)?
            .set_default("processing.validation_rules.temperature_max", 100.0)?
            .set_default("processing.validation_rules.humidity_min", 0.0)?
            .set_default("processing.validation_rules.humidity_max", 100.0)?
            .set_default("influxdb.host", "localhost")?
            .set_default("influxdb.port", 8086)?
            .set_default("influxdb.org", "emma")?
            .set_default("influxdb.bucket", "climate")?
            .set_default("influxdb.token", "emma-token")?
            .set_default("geocoder.geonames_file_path", "allCountries.txt")?
            // Override with environment variables
            .add_source(Environment::with_prefix("PROCESSOR").separator("_"))
            .build()?;

        config.try_deserialize()
    }
}
