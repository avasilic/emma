#!/bin/bash
set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Global variables for process tracking
INGESTOR_PID=""
PROCESSOR_PID=""
TEST_START_TIME=$(date +%s)

# Logging functions
log() {
    echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $1"
}

success() {
    echo -e "${GREEN}✅${NC} $1"
}

warning() {
    echo -e "${YELLOW}⚠️${NC} $1"
}

error() {
    echo -e "${RED}❌${NC} $1"
}

# Cleanup function - called on script exit
cleanup() {
    local exit_code=$?
    echo ""
    log "🧹 Starting cleanup process..."
    
    # Stop background processes gracefully
    if [[ -n "$INGESTOR_PID" ]]; then
        if kill -0 "$INGESTOR_PID" 2>/dev/null; then
            log "Stopping Go ingestor (PID: $INGESTOR_PID)..."
            kill -TERM "$INGESTOR_PID" 2>/dev/null || true
            sleep 3
            if kill -0 "$INGESTOR_PID" 2>/dev/null; then
                warning "Force killing ingestor..."
                kill -KILL "$INGESTOR_PID" 2>/dev/null || true
            fi
        fi
    fi
    
    if [[ -n "$PROCESSOR_PID" ]]; then
        if kill -0 "$PROCESSOR_PID" 2>/dev/null; then
            log "Stopping Rust processor (PID: $PROCESSOR_PID)..."
            kill -TERM "$PROCESSOR_PID" 2>/dev/null || true
            sleep 3
            if kill -0 "$PROCESSOR_PID" 2>/dev/null; then
                warning "Force killing processor..."
                kill -KILL "$PROCESSOR_PID" 2>/dev/null || true
            fi
        fi
    fi
    
    # Kill any remaining processes by pattern matching
    pkill -f "go run cmd/ingestor/main.go" 2>/dev/null || true
    pkill -f "cargo run.*processor" 2>/dev/null || true
    
    # Clean up log files
    rm -f ingestor.log processor.log 2>/dev/null || true
    
    # Clean up test configuration
    rm -f sources/test_weather.yaml 2>/dev/null || true
    
    # Calculate test duration
    local end_time=$(date +%s)
    local duration=$((end_time - TEST_START_TIME))
    
    echo ""
    if [[ $exit_code -eq 0 ]]; then
        success "Pipeline test completed successfully in ${duration}s"
    else
        error "Pipeline test failed after ${duration}s (exit code: $exit_code)"
    fi
    
    log "Cleanup completed"
    exit $exit_code
}

# Set up signal handlers
trap cleanup EXIT INT TERM

# Function to wait for service to be ready
wait_for_service() {
    local service_name="$1"
    local check_command="$2"
    local max_attempts=30
    local attempt=1
    
    log "Waiting for $service_name to be ready..."
    
    while [[ $attempt -le $max_attempts ]]; do
        if eval "$check_command" >/dev/null 2>&1; then
            success "$service_name is ready"
            return 0
        fi
        
        echo -n "."
        sleep 2
        ((attempt++))
    done
    
    error "$service_name failed to start within $((max_attempts * 2)) seconds"
    return 1
}

# Function to check if process is running
is_process_running() {
    local pid="$1"
    [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

# Main test function
main() {
    echo "🌍 Emma Climate Data Pipeline - Integration Test"
    echo "=============================================="
    echo ""
    
    # Step 1: Check prerequisites
    log "🔍 Checking prerequisites..."
    
    if ! command -v docker >/dev/null 2>&1; then
        error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    if ! command -v docker-compose >/dev/null 2>&1; then
        error "Docker Compose is not installed or not in PATH"
        exit 1
    fi
    
    if ! command -v go >/dev/null 2>&1; then
        error "Go is not installed or not in PATH"
        exit 1
    fi
    
    if ! command -v cargo >/dev/null 2>&1; then
        error "Rust/Cargo is not installed or not in PATH"
        exit 1
    fi
    
    success "All prerequisites found"
    
    # Step 2: Update dependencies
    log "📦 Updating Go dependencies..."
    go mod tidy
    success "Go dependencies updated"
    
    # Step 3: Generate protobuf files
    log "🔧 Generating protobuf files..."
    if command -v buf >/dev/null 2>&1; then
        buf generate
        success "Protobuf files generated with buf"
    else
        warning "buf not found, skipping protobuf generation"
    fi
    
    # Step 4: Start infrastructure
    log "🐳 Starting infrastructure (Kafka, InfluxDB)..."
    docker-compose up -d kafka influxdb
    
    # Wait for services
    wait_for_service "Kafka" "docker exec emma-kafka kafka-broker-api-versions --bootstrap-server localhost:9092"
    wait_for_service "InfluxDB" "curl -f http://localhost:8086/ping"
    
    # Step 5: Create Kafka topic
    log "📝 Creating Kafka topic..."
    docker exec emma-kafka kafka-topics --bootstrap-server localhost:9092 --create --topic data.raw --partitions 1 --replication-factor 1 --if-not-exists >/dev/null 2>&1
    success "Kafka topic 'data.raw' created"
    
    # Step 6: Create test source configuration
    log "📄 Creating test source configuration..."
    mkdir -p sources
    
    cat > sources/test_weather.yaml << 'EOF'
name: "test_weather_data"
type: "http_fetch"
category: "environmental"
frequency: "10s"
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
    
    success "Test source configuration created"
    
    # Step 7: Start Go ingestor
    log "🔄 Starting Go ingestor..."
    
    # Set environment variables
    export KAFKA_BROKERS="localhost:9092"
    export KAFKA_TOPIC="data.raw"
    
    # Start ingestor in background
    go run cmd/ingestor/main.go > ingestor.log 2>&1 &
    INGESTOR_PID=$!
    
    if is_process_running "$INGESTOR_PID"; then
        success "Go ingestor started (PID: $INGESTOR_PID)"
    else
        error "Failed to start Go ingestor"
        exit 1
    fi
    
    # Let ingestor run and generate some data
    log "⏳ Letting ingestor generate data for 30 seconds..."
    sleep 30
    
    # Check if ingestor is still running
    if ! is_process_running "$INGESTOR_PID"; then
        error "Ingestor process died unexpectedly"
        cat ingestor.log
        exit 1
    fi
    
    # Step 8: Verify Kafka messages
    log "📨 Checking Kafka messages..."
    local message_count
    message_count=$(docker exec emma-kafka kafka-console-consumer --bootstrap-server localhost:9092 --topic data.raw --from-beginning --timeout-ms 5000 2>/dev/null | wc -l || echo "0")
    
    if [[ $message_count -gt 0 ]]; then
        success "Found $message_count messages in Kafka"
    else
        error "No messages found in Kafka"
        exit 1
    fi
    
    # Step 9: Start Rust processor
    log "🦀 Starting Rust processor..."
    
    # Set up processor environment
    export PROCESSOR_KAFKA_BOOTSTRAP_SERVERS="localhost:9092"
    export PROCESSOR_KAFKA_TOPIC="data.raw"
    export PROCESSOR_INFLUXDB_HOST="localhost"
    export PROCESSOR_INFLUXDB_PORT="8086"
    export PROCESSOR_INFLUXDB_ORG="emma"
    export PROCESSOR_INFLUXDB_BUCKET="climate"
    export PROCESSOR_INFLUXDB_TOKEN="emma-token"
    
    # Build processor
    log "Building Rust processor..."
    (cd cmd/processor && cargo build --release)
    success "Rust processor built"
    
    # Start processor in background
    (cd cmd/processor && ./target/release/processor > ../../processor.log 2>&1) &
    PROCESSOR_PID=$!
    
    if is_process_running "$PROCESSOR_PID"; then
        success "Rust processor started (PID: $PROCESSOR_PID)"
    else
        error "Failed to start Rust processor"
        exit 1
    fi
    
    # Step 10: Let full pipeline run
    log "⏳ Running full pipeline for 60 seconds..."
    local count=0
    while [[ $count -lt 60 ]]; do
        # Check if processes are still running
        if ! is_process_running "$INGESTOR_PID"; then
            error "Ingestor process died unexpectedly"
            exit 1
        fi
        
        if ! is_process_running "$PROCESSOR_PID"; then
            error "Processor process died unexpectedly"
            exit 1
        fi
        
        echo -n "."
        sleep 1
        ((count++))
    done
    echo ""
    
    # Step 11: Verify results
    log "📊 Verifying pipeline results..."
    
    # Check InfluxDB data using v2 API
    local influx_result
    influx_result=$(curl -s -XPOST 'http://localhost:8086/api/v2/query?org=emma' \
        --header 'Authorization: Token emma-token' \
        --header 'Content-Type: application/vnd.flux' \
        --data 'from(bucket: "climate") |> range(start: -1h) |> filter(fn: (r) => r._measurement == "data_points") |> count()')
    
    if echo "$influx_result" | grep -q "_value"; then
        success "Data found in InfluxDB"
    else
        warning "No data found in InfluxDB, checking error logs..."
        echo "Processor logs:"
        tail -20 processor.log || true
        exit 1
    fi
    
    # Step 12: Display results
    echo ""
    log "📈 Pipeline Results:"
    echo "==================="
    
    echo ""
    echo "📨 Recent Kafka Messages (last 3):"
    docker exec emma-kafka kafka-console-consumer --bootstrap-server localhost:9092 --topic data.raw --from-beginning --max-messages 3 2>/dev/null || true
    
    echo ""
    echo "💾 InfluxDB Data (last 3 records):"
    curl -s -XPOST 'http://localhost:8086/api/v2/query?org=emma' \
        --header 'Authorization: Token emma-token' \
        --header 'Content-Type: application/vnd.flux' \
        --data 'from(bucket: "climate") |> range(start: -1h) |> filter(fn: (r) => r._measurement == "data_points") |> sort(columns: ["_time"], desc: true) |> limit(n: 3)' | jq '.' 2>/dev/null || curl -s -XPOST 'http://localhost:8086/api/v2/query?org=emma' \
        --header 'Authorization: Token emma-token' \
        --header 'Content-Type: application/vnd.flux' \
        --data 'from(bucket: "climate") |> range(start: -1h) |> filter(fn: (r) => r._measurement == "data_points") |> sort(columns: ["_time"], desc: true) |> limit(n: 3)'
    
    echo ""
    success "🎉 Complete pipeline test successful!"
    echo ""
    log "📋 Log files available:"
    echo "   - ingestor.log (Go ingestor output)"
    echo "   - processor.log (Rust processor output)"
    echo ""
    log "🌐 Services running:"
    echo "   - InfluxDB: http://localhost:8086"
    echo "   - Grafana: http://localhost:3000 (admin/admin)"
    echo "   - Kafka UI: http://localhost:8080"
    echo ""
    log "📊 To view data in Grafana:"
    echo "   1. Open http://localhost:3000"
    echo "   2. Login with admin/admin"
    echo "   3. Add InfluxDB datasource: http://influxdb:8086, org: emma, token: emma-token, bucket: climate"
    echo "   4. Create dashboards to visualize your climate data"
    echo ""
    log "🛑 To stop all services: docker compose down"
}

# Run main function
main "$@"
