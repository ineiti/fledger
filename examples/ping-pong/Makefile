RUN_WEB := true
PKILL = @/bin/ps aux | grep "$1" | egrep -v "(grep|vscode|rust-analyzer)" | awk '{print $$2}' | xargs -r kill

all: kill build run

build:
	cd ../../cli/flsignal && cargo build
	cd libc && cargo build
	cd wasm && trunk build

run: build
	cd ../../cli/flsignal && cargo run -- -v &
	sleep 2
	cd libc && cargo run &
	cd libc && cargo run &
	if [ "${RUN_WEB}" = "true" ]; then \
		cd wasm && trunk serve --open & \
	fi

kill:
	$(call PKILL,debug/flsignal)
	$(call PKILL,debug/libc)
	$(call PKILL,trunk)

test: RUN_WEB := false
test: run
	sleep 10
	$(MAKE) kill