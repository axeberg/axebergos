.PHONY: build dev clean check

# Build WASM package
build:
	wasm-pack build --target web --release

# Development build (faster, with debug info)
dev:
	wasm-pack build --target web --dev

# Run a local server for testing
serve: build
	python3 -m http.server 8080

# Type check without building
check:
	cargo check --target wasm32-unknown-unknown

# Clean build artifacts
clean:
	cargo clean
	rm -rf pkg/
