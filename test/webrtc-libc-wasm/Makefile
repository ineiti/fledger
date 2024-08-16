all: kill build run
PKILL = @ps aux | grep "$1" | grep -v grep | awk '{print $$2}' | xargs -r kill

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
	$(call PKILL,node ../run/main.js)
	$(call PKILL,cargo run)
	$(call PKILL,target/debug/webrtc-test-libc)

test:
	$(MAKE) build
	(sleep 5 ; $(MAKE) wasm) &
	$(MAKE) libc
	sleep 5
	$(MAKE) kill
