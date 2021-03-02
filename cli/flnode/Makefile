src := $(shell find src ../../common ../../wasm/lib -name "*.rs")
wasm := static/wasm_bg.wasm
build = wasm-pack build --debug --target nodejs --out-name wasm \
	--out-dir ./static -- $(FEATURES)

run: build_local
	node run/main.js

run2: build_local
	mkdir -p node[12]
	cd node1 && node ../run/main.js &
	cd node2 && node ../run/main.js

run_remote: build
	cd run; npm ci
	node run/main.js

build: ${wasm}

${wasm}: ${src}
	${build}

build_local: FEATURES=--features local
build_local: build
	cd run; npm ci
