miniserve := $(shell echo $$HOME )/.cargo/bin/miniserve
src := $(shell find src -name "*.rs")
build := wasm-pack build --debug --target web --out-name wasm --out-dir ./static

serve: static/wasm_bg.wasm ${miniserve}
	${miniserve} ./static --index index.html

${miniserve}:
	cargo install miniserve

static/wasm_bg.wasm: ${src}
	${build}

.PHONY: watch build_bg
watch: build_bg serve

build:
	${build}

