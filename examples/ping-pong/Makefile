RUN_WEB := true
PKILL = @ps aux | grep "$1" | grep -v grep | awk '{print $$2}' | xargs -r kill

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
	$(call PKILL,debug/flsignal)
	$(call PKILL,debug/libc)
	$(call PKILL,trunk)

test: RUN_WEB := false
test: run
	sleep 10
	$(MAKE) kill