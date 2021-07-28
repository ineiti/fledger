WASM_FLAGS :=

build:
	wasm-pack build -- ${WASM_FLAGS}
	cd www && ( npm ci && npm run build && cp main.css dist )

clean:
	rm -rf www/dist

build_local: WASM_FLAGS = --features local
build_local: build

www/dist: build_local

serve: www/dist
	cd www && npm run start
