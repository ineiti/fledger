miniserve := $(shell echo $$HOME )/.cargo/bin/miniserve
wasm := static/wasm_bg.wasm
src := $(shell find ../../common ../lib src -name "*.rs")
TARGET := --release
build = wasm-pack build ${TARGET} --target web --out-name wasm \
	--out-dir ./static -- $(FEATURES)

serve: TARGET := --debug
serve: ${wasm} ${miniserve}
	${miniserve} ./static --index index.html

${miniserve}:
	cargo install miniserve

${wasm}: ${src}
	${build}

.PHONY: watch build_bg
watch: build_bg serve

build: ${wasm}

build_local: FEATURES=--features local
build_local: TARGET=--debug
build_local: build
