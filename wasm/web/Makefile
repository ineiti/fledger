miniserve := $(shell echo $$HOME )/.cargo/bin/miniserve
wasm := static/wasm_bg.wasm
src := $(shell find src -name "*.rs")
build = wasm-pack build --debug --target web --out-name wasm \
	--out-dir ./static -- $(FEATURES)

serve: ${wasm} ${miniserve}
	${miniserve} ./static --index index.html

${miniserve}:
	cargo install miniserve

${wasm}: ${src}
	${build}

.PHONY: watch build_bg
watch: build_bg serve

build: ${wasm}
	${build}

build_local: FEATURES=--features local
build_local: build
