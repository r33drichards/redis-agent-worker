.PHONY: build run test test-integration test-verbose test-ci clean install dev redis-up redis-down enqueue stats

# Build the project in release mode
build:
	cargo build --release

# Build in debug mode
dev:
	cargo build

# Run the worker
run:
	cargo run -- run

# Run all tests
test:
	cargo test

# Run integration tests only
test-integration:
	cargo test --test integration_test

# Run tests with output
test-verbose:
	cargo test -- --nocapture --test-threads=1

# Run tests with debug logging
test-debug:
	RUST_LOG=debug cargo test -- --nocapture

# Run tests suitable for CI (with logging)
test-ci:
	RUST_LOG=info cargo test -- --nocapture

# Clean build artifacts
clean:
	cargo clean
	rm -rf work/

# Install the binary
install: build
	cp target/release/redis-agent-worker /usr/local/bin/

# Start Redis with Docker Compose
redis-up:
	docker-compose up -d

# Stop Redis
redis-down:
	docker-compose down

# View Redis data
redis-cli:
	docker-compose exec redis redis-cli

# Enqueue a test job
enqueue:
	cargo run -- enqueue \
		--job-id "test-$$(date +%s)" \
		--repo-url "git@github.com:user/repo.git" \
		--branch "main" \
		--prompt "Run tests and fix any errors"

# Show queue statistics
stats:
	cargo run -- stats

# Peek at next job
peek:
	cargo run -- peek

# Recover stalled jobs
recover:
	cargo run -- recover

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy -- -D warnings

# Run in watch mode (requires cargo-watch)
watch:
	cargo watch -x run

# Full development setup
setup: redis-up
	@echo "Redis is starting..."
	@sleep 3
	@echo "Setup complete! Redis is running on localhost:6379"
	@echo "Redis Commander is available at http://localhost:8081"
