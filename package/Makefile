.PHONY: build build-release clean dev pack publish

dev: build

build:
	wasm-pack build --target web --out-name danu --out-dir pkg --scope fledger --features local

build-release:
	wasm-pack build --target web --out-name danu --out-dir pkg --scope fledger --release

clean:
	rm -rf pkg/
	cargo clean

pack: build
	npm pack

publish: build-release
	wasm-pack publish --access public

test:
	wasm-pack test --firefox --headless
