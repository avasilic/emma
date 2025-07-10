// cmd/ingestor/main.go
package main

import (
	"fmt"
	"log"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"emma/internal/config"
	"emma/internal/handlers"
	"emma/internal/kafka"
)

func main() {
	log.Println("Starting Data Ingestor...")

	// Load source configurations
	sources, err := config.LoadSourceConfigs("./sources/")
	if err != nil {
		log.Fatalf("Failed to load source configs: %v", err)
	}

	if len(sources) == 0 {
		log.Fatal("No source configurations found in ./sources/")
	}

	log.Printf("Loaded %d source configurations", len(sources))

	// Initialize Kafka producer
	kafkaConfig := kafka.ProducerConfig{
		Brokers: getKafkaBrokers(),
		Topic:   getKafkaTopic(),
	}

	producer, err := kafka.NewProducer(kafkaConfig)
	if err != nil {
		log.Fatalf("Failed to create Kafka producer: %v", err)
	}
	defer producer.Close()

	// Start workers for each source
	for _, source := range sources {
		go startWorker(source, producer)
	}

	// Wait for interrupt signal
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

	log.Println("✅ Data Ingestor started successfully")
	log.Println("Press Ctrl+C to stop...")

	<-sigChan
	log.Println("🛑 Shutting down gracefully...")
}

func startWorker(source config.SourceConfig, producer *kafka.Producer) {
	log.Printf("Starting worker for source: %s (type: %s, category: %s)", source.Name, source.Type, source.Category)

	// Get handler for this source type
	handler, err := handlers.GetHandler(source.Type)
	if err != nil {
		log.Printf("Failed to get handler for %s: %v", source.Name, err)
		return
	}

	// Validate configuration
	if err := handler.Validate(source.Config); err != nil {
		log.Printf("Invalid config for %s: %v", source.Name, err)
		return
	}

	// Parse frequency
	frequency, err := source.GetFrequency()
	if err != nil {
		log.Printf("Invalid frequency for %s: %v", source.Name, err)
		return
	}

	// Add category to config for handler
	sourceConfig := make(map[string]interface{})
	for k, v := range source.Config {
		sourceConfig[k] = v
	}
	sourceConfig["category"] = source.Category
	sourceConfig["source"] = source.Name

	// Start periodic fetching
	ticker := time.NewTicker(frequency)
	defer ticker.Stop()

	// Fetch immediately on start
	fetchAndPublish(source.Name, handler, sourceConfig, producer)

	// Then fetch on schedule
	for range ticker.C {
		fetchAndPublish(source.Name, handler, sourceConfig, producer)
	}
}

func fetchAndPublish(sourceName string, handler handlers.Handler, config map[string]interface{}, producer *kafka.Producer) {
	log.Printf("Fetching data for source: %s", sourceName)

	points, err := handler.Fetch(config)
	if err != nil {
		log.Printf("Error fetching data for %s: %v", sourceName, err)
		return
	}

	if len(points) == 0 {
		log.Printf("No data points received for %s", sourceName)
		return
	}

	log.Printf("Successfully fetched %d data points for %s", len(points), sourceName)

	// Print the data (for debugging)
	for _, point := range points {
		fmt.Printf("📊 %s (%s): %s = %.2f %s at (%.4f, %.4f)\n",
			point.Source, point.Category, point.Variable, point.Value, point.Units, point.Lat, point.Lon)
	}

	// Publish to Kafka
	err = producer.PublishPoints(points)
	if err != nil {
		log.Printf("Failed to publish data for %s: %v", sourceName, err)
		return
	}

	log.Printf("✅ Successfully published %d points for %s", len(points), sourceName)
}

func getKafkaBrokers() []string {
	brokers := os.Getenv("KAFKA_BROKERS")
	if brokers == "" {
		brokers = "localhost:9092"
	}
	return strings.Split(brokers, ",")
}

func getKafkaTopic() string {
	topic := os.Getenv("KAFKA_TOPIC")
	if topic == "" {
		topic = "data.raw"
	}
	return topic
}
