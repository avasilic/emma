use anyhow::Result;
use tokio_stream::StreamExt;
use tracing::{error, info};

mod config;
mod influx_writer;
mod kafka_consumer;
mod processor;
mod proto;

use config::ProcessorConfig;
use influx_writer::InfluxWriter;
use kafka_consumer::KafkaConsumer;
use processor::DataProcessor;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🦀 Starting Climate Data Processor...");

    // Load configuration
    let config = ProcessorConfig::load()?;
    info!("📝 Loaded configuration");

    // Initialize components
    let kafka_consumer = KafkaConsumer::new(&config.kafka)?;
    let processor = DataProcessor::new(&config.processing);
    let influx_writer = InfluxWriter::new(&config.influxdb).await?;

    info!("🔌 Connected to Kafka and InfluxDB");

    // Start processing pipeline
    let mut message_stream = kafka_consumer.stream().await?;

    info!("🚀 Processing pipeline started");

    // Main processing loop
    while let Some(message) = message_stream.next().await {
        match message {
            Ok(climate_point) => {
                // Process the data point
                match processor.process(climate_point).await {
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
