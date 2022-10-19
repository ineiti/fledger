RUN_WEB := true

all: kill build run

build:
	cd ../../cli/flsignal && cargo build
	cd libc && cargo build
	cd wasm && trunk build

run: build
	cd ../../cli/flsignal && cargo run -- -vv &
	sleep 2
	cd libc && cargo run &
	cd libc && cargo run &
	if [ "${RUN_WEB}" = "true" ]; then \
		cd wasm && trunk serve --open & \
	fi

kill:
	-pkill -f debug/flsignal
	-pkill -f debug/libc
	-pkill -f trunk

test: RUN_WEB := false
test: run
	sleep 10
	$(MAKE) kill