all: kill build run

run:
	$(MAKE) libc &
	sleep 2
	$(MAKE) wasm &

.PHONY: libc wasm

libc:
	cd libc && cargo run .

wasm:
	make -C wasm run

build:
	cd libc && cargo build
	make -C wasm build

kill:
	@-pkill -f "node ../run/main.js"
	@-pkill -f "cargo run"
	@-pkill -f "target/debug/webrtc-test-libc"

test:
	$(MAKE) build
	(sleep 5 ; $(MAKE) wasm) &
	$(MAKE) libc
	$(MAKE) kill