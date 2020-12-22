miniserve := $(shell echo $$HOME )/.cargo/bin/miniserve
src := $(shell find src -name "*.rs")

serve: static/wasm_bg.wasm ${miniserve}
	${miniserve} ./static --index index.html

${miniserve}:
	cargo install miniserve

static/wasm_bg.wasm: ${src}
	wasm-pack build --target web --out-name wasm --out-dir ./static
