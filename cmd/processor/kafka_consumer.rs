use anyhow::{anyhow, Result};
use futures::stream::BoxStream;
use prost::Message as ProstMessage;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::BorrowedMessage;
use rdkafka::{ClientConfig, Message};
use tokio_stream::StreamExt;
use tracing::{error, info};

use crate::config::KafkaConfig;
use crate::proto::ClimatePoint;

// Include generated protobuf code

pub struct KafkaConsumer {
    consumer: StreamConsumer,
    topic: String,
}

impl KafkaConsumer {
    pub fn new(config: &KafkaConfig) -> Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &config.bootstrap_servers)
            .set("group.id", &config.group_id)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", &config.auto_offset_reset)
            .set("enable.partition.eof", "false")
            .create()
            .map_err(|e| anyhow!("Failed to create Kafka consumer: {}", e))?;

        Ok(KafkaConsumer {
            consumer,
            topic: config.topic.clone(),
        })
    }

    pub async fn stream(&self) -> Result<BoxStream<'_, Result<ClimatePoint>>> {
        self.consumer
            .subscribe(&[&self.topic])
            .map_err(|e| anyhow!("Failed to subscribe to topic {}: {}", self.topic, e))?;

        info!("📡 Subscribed to Kafka topic: {}", self.topic);

        let message_stream = self
            .consumer
            .stream()
            .map(|message_result| match message_result {
                Ok(message) => self.parse_message(message),
                Err(e) => {
                    error!("Kafka message error: {}", e);
                    Err(anyhow!("Kafka message error: {}", e))
                }
            });

        Ok(Box::pin(message_stream))
    }

    fn parse_message(&self, message: BorrowedMessage) -> Result<ClimatePoint> {
        let payload = message
            .payload()
            .ok_or_else(|| anyhow!("Message has no payload"))?;

        // Decode as protobuf
        let climate_point = ClimatePoint::decode(payload)
            .map_err(|e| anyhow!("Failed to decode protobuf message: {}", e))?;

        info!(
            "📊 Received: {} = {:.2} {} from {} at ({:.4}, {:.4})",
            climate_point.variable,
            climate_point.value,
            climate_point.units,
            climate_point.source,
            climate_point.lat,
            climate_point.lon
        );

        Ok(climate_point)
    }
}
