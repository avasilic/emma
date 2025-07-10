use anyhow::Result;
use tokio_stream::StreamExt;
use tracing::{error, info};

mod config;
mod geo;
mod influx_writer;
mod kafka_consumer;
mod processor;
mod proto;

use geo::H3Geocoder;

use config::ProcessorConfig;
use influx_writer::InfluxWriter;
use kafka_consumer::KafkaConsumer;
use processor::DataProcessor;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("🦀 Starting Data Processor...");

    let config = ProcessorConfig::load()?;
    info!("📝 Loaded configuration");

    // Initialize H3 geocoder first
    info!(
        "🗺️ Loading geo location data from: {}",
        config.geocoder.geonames_file_path
    );
    let geocoder = match H3Geocoder::from_geonames_file(&config.geocoder.geonames_file_path) {
        Ok(geocoder) => {
            info!("✅ Geo location data loaded successfully");
            geocoder
        }
        Err(e) => {
            error!("❌ Failed to load geo location data: {}", e);
            return Err(anyhow::anyhow!("Failed to load geo location data: {}", e));
        }
    };

    // Initialize components
    let kafka_consumer = KafkaConsumer::new(&config.kafka)?;
    let processor = DataProcessor::new(&config.processing, geocoder);
    let influx_writer = InfluxWriter::new(&config.influxdb).await?;

    info!("🔌 Connected to Kafka and InfluxDB");

    let mut message_stream = kafka_consumer.stream().await?;

    info!("🚀 Processing pipeline started");

    // Main processing loop
    while let Some(message) = message_stream.next().await {
        match message {
            Ok(data_point) => {
                // Process the data point
                match processor.process(data_point).await {
                    Ok(processed_points) => {
                        // Write to InfluxDB
                        if let Err(e) = influx_writer.write_points(processed_points).await {
                            error!("Failed to write to InfluxDB: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to process data point: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Kafka consumer error: {}", e);
            }
        }
    }

    Ok(())
}
