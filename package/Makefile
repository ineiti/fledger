.PHONY: build build-release clean dev pack

dev: build

build:
	wasm-pack build --target web --out-name flbrowser_core --out-dir pkg --features local

build-release:
	wasm-pack build --target web --out-name flbrowser_core --out-dir pkg --release

clean:
	rm -rf pkg/
	cargo clean

pack: build
	npm pack
