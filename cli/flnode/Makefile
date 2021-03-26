src := $(shell find src ../../common ../../wasm/lib ../../vendor -name "*.rs")
wasm := static/wasm_bg.wasm
build = wasm-pack build --debug --target nodejs --out-name wasm \
	--out-dir ./static -- $(FEATURES)

clean:
	rm $(wasm)

run: build_local run/node_modules
	node run/main.js

run2: build_local run/node_modules
	mkdir -p node{1,2}
	cd node1 && node ../run/main.js &
	cd node2 && node ../run/main.js

run_remote: build run/node_modules
	node run/main.js

build: ${wasm}

run/node_modules:
	cd run; npm ci

${wasm}: ${src}
	${build}

build_local: FEATURES=--features local
build_local: build
