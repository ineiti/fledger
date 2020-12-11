static/wasm_bg.wasm: src/lib.rs
	wasm-pack build --target web --out-name wasm --out-dir ./static

serve: static/wasm_bg.wasm
	miniserve ./static --index index.html
