CLIs := fledger flsignal
CLI_PATHS := $(patsubst %,cli/%,$(CLIs))
CARGOS := $(CLI_PATHS) flarch flbrowser flcrypto flmacro \
			flmodules flnode test/{signal-fledger,fledger-nodejs,webrtc-libc-wasm/{libc,wasm}} \
			examples/ping-pong/{shared,libc,wasm}
CARGOS_NOWASM := $(CLI_PATHS) flarch flbrowser flcrypto flmacro \
			flmodules flnode test/{signal-fledger,webrtc-libc-wasm/libc} \
			examples/ping-pong/{shared,libc}
CARGO_LOCKS := . test/{fledger-nodejs,webrtc-libc-wasm/wasm} flbrowser examples/ping-pong/wasm

MAKE_TESTS := examples/ping-pong test/webrtc-libc-wasm
CRATES := flcrypto flmacro flarch flmodules flnode
SHELL := /bin/bash
PKILL = @/bin/ps aux | grep "$1" | egrep -v "(grep|vscode|rust-analyzer)" | awk '{print $$2}' | xargs -r kill
PUBLISH = --token $$CARGO_REGISTRY_TOKEN
PUBLISH_EXCLUDE := --exclude "test-*" --exclude "examples-*"
FLEDGER = cargo run --bin fledger -- --config fledger/test0$1 --name Local0$1 \
		--log-dht-storage -s ws://localhost:8765 --disable-turn-stun

cargo_check:
	set -e; \
	for c in ${CARGO_LOCKS}; do \
	  echo Checking $$c; \
	  ( cd $$c && cargo check --tests ) || exit 1; \
	done

cargo_test:
	set -e; \
	for c in ${CARGO_LOCKS}; do \
	  echo Checking $$c; \
	  ( cd $$c && cargo test ) || exit 1; \
	done

make_test:
	echo "Tests which need to be inspected manually. But still run them in github workflows."
	for c in ${MAKE_TESTS}; do \
	  ( cd $$c && make test ) \
	done

cargo_update:
	for c in ${CARGO_LOCKS}; do \
		echo Updating $$c; \
		(cd $$c && cargo update ); \
	done

cargo_clean:
	for c in ${CARGO_LOCKS}; do \
		echo Cleaning $$c; \
		(cd $$c && cargo clean ); \
	done

cargo_build:
	set -e; \
	for c in ${CARGO_LOCKS}; do \
		echo Building $$c; \
		(cd $$c && cargo build --tests ) || exit 1; \
	done

cargo_unused:
	for cargo in ${CARGOS_NOWASM}; do \
		echo Checking for unused crates in $$cargo; \
		(cd $$(dirname $$cargo) && cargo +nightly udeps -q --all-targets ); \
	done

publish_dry: PUBLISH = --dry-run
publish_dry: publish

publish:
	set -e; \
	for crate in ${CRATES}; do \
		if grep -q '"\*"' $$crate/Cargo.toml; then \
			echo "Remove wildcard version from $$crate"; \
			exit 1; \
		fi; \
	done
	cargo-workspaces workspaces publish ${PUBLISH} --publish-as-is

update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > flnode/src/version.rs

kill:
	$(call PKILL,flsignal)
	$(call PKILL,fledger)
	$(call PKILL,trunk serve)

build_cli:
	cd cli && \
	for cli in ${CLIs}; do \
	  echo "Building $$cli"; \
	  cargo build $(CARGO_FLAGS) -p $$cli; \
	done

build_cli_release: CARGO_FLAGS = --release
build_cli_release: build_cli

build_web_release:
	cd flbrowser && trunk build --release

build_web:
	cd flbrowser && trunk build

build_servers: build_cli build_web

build_local_web:
	cd flbrowser && trunk build --features local

build_local: build_local_web build_cli

serve_two: kill build_cli
	( cd cli && cargo run --bin flsignal -- -v ) &
	sleep 4
	( cd cli && ( $(call FLEDGER,1) & $(call FLEDGER,2) & ) )

node_1:
	cd cli && $(call FLEDGER,1)

node_2:
	cd cli && $(call FLEDGER,2)

serve_local: kill build_local_web serve_two
	cd flbrowser && RUST_BACKTRACE=1 trunk serve --features local -w . -w ../flmodules &
	sleep 2
	open http://localhost:8080

create_local_realm:
	cd cli/fledger && cargo run -- -c test03 -n Local03 -s ws://localhost:8765 --disable-turn-stun realm create danu_realm

docker_dev:
	for cli in ${CLIs}; do \
		docker build --target $$cli --platform linux/amd64 -t fledgre/$$cli:dev . -f Dockerfile.dev --progress plain; \
		docker push fledgre/$$cli:dev; \
	done

clean:
	for c in ${CARGOS}; do \
		echo "Cleaning $$c"; \
		( cd $$c && cargo clean ) ; \
	done
