.PHONY: build run dev test clean fmt lint check release docker-build docker-up docker-down docker-logs

# Build
build:
	cargo build

release:
	cargo build --release

# Run
run:
	cargo run -- serve

dev:
	cargo run -- serve --config config.yaml

# Cache management
purge-all:
	cargo run -- purge --all

stats:
	cargo run -- stats

# Development
fmt:
	cargo fmt

lint:
	cargo clippy

check:
	cargo check

test:
	cargo test

# Clean
clean:
	cargo clean

# Docker
docker-build:
	docker compose build

docker-up:
	docker compose up -d

docker-down:
	docker compose down

docker-logs:
	docker compose logs -f
