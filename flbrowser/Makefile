WASM_FLAGS =
SRC_FILES := $(shell find ../fl{arch,modules,net,node}/src src -name "*.rs" )
WASM := pkg/fledger_web_bg.wasm

build: .built $(WASM)

.built:
	rm -f .built_local $(WASM)
	touch .built

$(WASM): $(SRC_FILES)
	wasm-pack build -- $(WASM_FLAGS)
	cd www && ( npm ci && npm run build && cp main.css bootstrap.min.css dist )

clean:
	rm -rf www/dist pkg

build_local: WASM_FLAGS = --features local
build_local: .built_local
build_local: $(WASM)

.built_local:
	rm -f .built $(WASM)
	touch .built_local

serve_local: build_local
	cd www && npm run start

serve: build
	cd www && npm run start
