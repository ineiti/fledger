all:
	$(MAKE) libc &
	$(MAKE) wasm &

.PHONY: libc wasm

libc:
	cd libc && cargo run .

wasm:
	make -C wasm run
