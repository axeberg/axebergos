.PHONY: build dev clean check serve docs docs-serve

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

# Build documentation with Zensical
docs:
	zensical build

# Serve documentation locally with hot reload
docs-serve:
	zensical serve

# Clean build artifacts
clean:
	cargo clean
	rm -rf pkg/ site/
