all:
	$(MAKE) libc &
	$(MAKE) wasm &

libc:
	cd libc && cargo run .

wasm:
	make -C wasm run
