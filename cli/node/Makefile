WEB = ../../wasm/web

run: $(WEB)/static/wasm_bg.wasm
	make -C $(WEB) serve &
	cargo run

$(WEB)/static/wasm_bg.wasm:
	make -C $(WEB) build

run_headless:
	/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
		--headless --disable-gpu --remote-debugging-port=9223 \
		--user-data-dir="./cache" http://localhost:8080
