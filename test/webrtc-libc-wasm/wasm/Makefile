src := $(shell find src ../../../shared/flnet ../../../wasm/flnet-wasm ../../../shared/flutils -name "*.rs")
wasm := static/wasm_bg.wasm
TARGET := --debug
build = wasm-pack build ${TARGET} --target nodejs --out-name wasm \
	--out-dir ./static -- $(FEATURES)

clean:
	rm -f $(wasm)

run: build
	mkdir -p node1
	cd node1 && node ../run/main.js

build: ${wasm} run/node_modules

run/node_modules:
	cd run && npm i @mapbox/node-pre-gyp && npm ci

${wasm}: ${src}
	${build}
