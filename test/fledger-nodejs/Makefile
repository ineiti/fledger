src := $(shell find src ../../fl* -name "*.rs") Cargo.toml
wasm := static/wasm_bg.wasm
TARGET := --release
build = wasm-pack build ${TARGET} --target nodejs --out-name wasm \
	--out-dir ./static -- $(FEATURES)
PKILL = @ps aux | grep "$1" | grep -v grep | awk '{print $$2}' | xargs -r kill

clean:
	rm -f $(wasm)

runa: build_local 
	mkdir -p node1
	cd node1 && node ../run/main.js

runb: build_local 
	mkdir -p node2
	cd node2 && node ../run/main.js

runc: build_local 
	node run/main.js

run2:
	$(MAKE) runa &
	sleep .5
	$(MAKE) runb

run_remote: build
	node run/main.js

build: ${wasm} run/node_modules

run/node_modules:
	cd run && npm i @mapbox/node-pre-gyp && npm ci

${wasm}: ${src}
	${build}

kill:
	$(call PKILL,node ../)

build_local: FEATURES=--features local
build_local: TARGET=--debug
build_local: build run/node_modules
