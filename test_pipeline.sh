#!/bin/bash

echo "🌍 Testing Complete Climate Data Pipeline..."
echo ""

# Step 1: Update Go dependencies
echo "📦 Updating Go dependencies..."
go mod tidy

# Step 2: Start infrastructure
echo "🐳 Starting infrastructure (Kafka, InfluxDB)..."
docker-compose up -d kafka influxdb

# Wait for services to be ready
echo "⏳ Waiting for services to start..."
sleep 10

# Step 3: Create test source configuration
echo "📄 Creating test source configuration..."
mkdir -p sources

cat > sources/test_weather.yaml << 'EOF'
name: "test_weather_data"
type: "http_fetch"
frequency: "15s"
config:
  url: "https://httpbin.org/json"
  method: "GET"
  response_path: "slideshow.author"
  variable: "temperature"
  units: "celsius"
  source: "test_api"
  coordinates:
    lat: 35.6762
    lon: 139.6503
EOF

echo "✅ Test source created"

# Step 4: Test Go ingestor
echo "🔄 Testing Go ingestor..."
echo "Starting ingestor in background..."

# Set environment variables
export KAFKA_BROKERS="localhost:9092"
export KAFKA_TOPIC="weather.raw"

# Start ingestor in background
go run cmd/ingestor/main.go &
INGESTOR_PID=$!

# Let it run for a bit
sleep 30

# Step 5: Test Rust processor
echo "🦀 Testing Rust processor..."
cd cmd/processor

# Set up processor environment
export PROCESSOR_KAFKA_BOOTSTRAP_SERVERS="localhost:9092"
export PROCESSOR_KAFKA_TOPIC="weather.raw"
export PROCESSOR_INFLUXDB_HOST="localhost"
export PROCESSOR_INFLUXDB_PORT="8086"
export PROCESSOR_INFLUXDB_DATABASE="climate"

# Build and run processor
cargo build
echo "Starting processor..."
cargo run &
PROCESSOR_PID=$!

# Let the full pipeline run
sleep 60

# Step 6: Check results
echo "📊 Checking results..."
echo "Kafka topic messages:"
docker exec -it emma-kafka-1 kafka-console-consumer.sh --bootstrap-server localhost:9092 --topic weather.raw --from-beginning --max-messages 5

echo ""
echo "InfluxDB data:"
curl -G 'http://localhost:8086/query' --data-urlencode "db=climate" --data-urlencode "q=SELECT * FROM climate_data LIMIT 5"

# Step 7: Cleanup
echo "🧹 Cleaning up..."
kill $INGESTOR_PID 2>/dev/null || true
kill $PROCESSOR_PID 2>/dev/null || true

echo ""
echo "✅ Pipeline test completed!"
echo "📈 Check Grafana at http://localhost:3000 to visualize the data"
echo "🗄️  Check InfluxDB at http://localhost:8086 for raw data"
echo ""
echo "To stop infrastructure: docker-compose down"