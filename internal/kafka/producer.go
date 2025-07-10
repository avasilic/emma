// internal/kafka/producer.go
package kafka

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"time"

	"emma/gen/go/proto/v1"
	"github.com/segmentio/kafka-go"
	protobuf "google.golang.org/protobuf/proto"
)

type Producer struct {
	writer *kafka.Writer
	topic  string
}

type ProducerConfig struct {
	Brokers []string
	Topic   string
}

func NewProducer(config ProducerConfig) (*Producer, error) {
	writer := &kafka.Writer{
		Addr:         kafka.TCP(config.Brokers...),
		Topic:        config.Topic,
		Balancer:     &kafka.LeastBytes{},
		WriteTimeout: 10 * time.Second,
		ReadTimeout:  10 * time.Second,
	}

	// Connection will be validated on first real message

	log.Printf("✅ Connected to Kafka brokers: %v, topic: %s", config.Brokers, config.Topic)

	return &Producer{
		writer: writer,
		topic:  config.Topic,
	}, nil
}

func (p *Producer) PublishPoints(points []*v1.DataPoint) error {
	if len(points) == 0 {
		return nil
	}

	messages := make([]kafka.Message, len(points))

	for i, point := range points {
		// Try protobuf first
		data, err := protobuf.Marshal(point)
		if err != nil {
			// Fallback to JSON
			log.Printf("⚠️  Failed to marshal protobuf, falling back to JSON: %v", err)
			data, err = json.Marshal(point)
			if err != nil {
				return fmt.Errorf("failed to marshal point to JSON: %w", err)
			}
		}

		messages[i] = kafka.Message{
			Key:   []byte(point.Source),
			Value: data,
			Headers: []kafka.Header{
				{Key: "source", Value: []byte(point.Source)},
				{Key: "variable", Value: []byte(point.Variable)},
				{Key: "category", Value: []byte(point.Category)},
				{Key: "format", Value: []byte("protobuf")},
			},
		}
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	err := p.writer.WriteMessages(ctx, messages...)
	if err != nil {
		return fmt.Errorf("failed to write messages to Kafka: %w", err)
	}

	log.Printf("📤 Published %d messages to Kafka topic: %s", len(messages), p.topic)
	return nil
}

func (p *Producer) PublishPoint(point *v1.DataPoint) error {
	return p.PublishPoints([]*v1.DataPoint{point})
}

func (p *Producer) Close() error {
	return p.writer.Close()
}
