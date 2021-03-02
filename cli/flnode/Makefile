src := $(shell find src ../../common ../../wasm/lib -name "*.rs")
wasm := static/wasm_bg.wasm
build = wasm-pack build --debug --target nodejs --out-name wasm \
	--out-dir ./static -- $(FEATURES)

run: build_local
	cd static; npm ci
	node run/main.js

build: ${wasm}

${wasm}: ${src}
	${build}

build_local: FEATURES=--features local
build_local: build
