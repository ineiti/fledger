all: kill build run

build:
	cd ../../cli/flsignal && cargo build
	cd libc && cargo build
	cd wasm && trunk build

run:
	cd ../../cli/flsignal && cargo run -- -vv &
	sleep 2
	cd libc && cargo run &
	cd wasm && trunk serve --open

kill:
	-pkill -f debug/flsignal
	-pkill -f debug/libc