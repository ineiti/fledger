.PHONY: build build-release clean dev pack publish

dev: build

build:
	wasm-pack build --target web --out-name danu --out-dir pkg --scope fledger --features local
	npm run build:ts

build-release:
	wasm-pack build --target web --out-name danu --out-dir pkg --scope fledger --release
	npm run build:ts

clean:
	rm -rf pkg/ dist/ node_modules/
	cargo clean

pack: build-release
	npm pack

publish: build-release
	npm publish --access public

test:
	wasm-pack test --firefox --headless
