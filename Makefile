.PHONY: build dev clean check serve

# Build WASM package
build:
	wasm-pack build --target web --release

# Development build (faster, with debug info)
dev:
	wasm-pack build --target web --dev

# Run the built-in dev server
serve: build
	cargo run --bin serve

# Type check without building (lib only, serve bin doesn't compile for wasm)
check:
	cargo check --lib --target wasm32-unknown-unknown

# Clean build artifacts
clean:
	cargo clean
	rm -rf pkg/
