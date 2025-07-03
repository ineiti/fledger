CARGOS := cli/{fledger,flsignal} flarch flbrowser flcrypto flmacro \
			flmodules flnode test/{signal-fledger,fledger-nodejs,webrtc-libc-wasm/{libc,wasm}} \
			examples/ping-pong/{shared,libc,wasm}
CARGOS_NOWASM := cli/{fledger,flsignal} flarch flbrowser flcrypto flmacro \
			flmodules flnode test/{signal-fledger,webrtc-libc-wasm/libc} \
			examples/ping-pong/{shared,libc}
CARGO_LOCKS := . test/{fledger-nodejs,webrtc-libc-wasm/wasm} flbrowser examples/ping-pong/wasm

MAKE_TESTS := examples/ping-pong test/webrtc-libc-wasm
CRATES := flcrypto flmacro flarch flmodules flnode
SHELL := /bin/bash
PKILL = @/bin/ps aux | grep "$1" | egrep -v "(grep|vscode|rust-analyzer)" | awk '{print $$2}' | xargs -r kill
PUBLISH = --token $$CARGO_REGISTRY_TOKEN
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
		CRATE_VERSION=$$(cargo search $$crate | grep "^$$crate " | sed -e "s/.*= \"\(.*\)\".*/\1/"); \
		CARGO_VERSION=$$(grep "^version" $$crate/Cargo.toml | head -n 1 | sed -e "s/.*\"\(.*\)\".*/\1/"); \
		if [[ "$$CRATE_VERSION" != "$$CARGO_VERSION" ]]; then \
			echo "Publishing crate $$crate"; \
			cargo publish ${PUBLISH} --manifest-path $$crate/Cargo.toml; \
		fi; \
	done

update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > flnode/src/version.rs

kill:
	$(call PKILL,flsignal)
	$(call PKILL,fledger)
	$(call PKILL,trunk serve)

build_cli:
	cd cli && cargo build -p fledger && cargo build -p flsignal

build_cli_release:
	cd cli && cargo build --release -p fledger && cargo build --release -p flsignal

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

serve_local: kill build_local_web serve_two
	cd flbrowser && RUST_BACKTRACE=1 trunk serve --features local -w . -w ../flmodules &
	sleep 2
	open http://localhost:8080

create_local_realm:
	cd cli/fledger && cargo run -- -c test03 -n Local03 -s ws://localhost:8765 --disable-turn-stun realm create danu_realm

docker_dev:
	for cli in fledger flsignal; do \
		docker build --target $$cli --platform linux/amd64 -t fledgre/$$cli:dev . -f Dockerfile.dev --progress plain; \
		docker push fledgre/$$cli:dev; \
	done

deploy:
	@test "${DEVBOX_SHELL_ENABLED}" == "1" || (echo "Devbox not enabled. Aborting..."; exit 1)
	@echo "Building fledger with musl, then deploying to XDC..."
	cd cli && cargo build --release --target=x86_64-unknown-linux-musl -p fledger && cargo build --release --target=x86_64-unknown-linux-musl -p flsignal
	@./deploy-binaries.sh

test_realm:
	/usr/bin/mktemp -d -t fledger-XXXX
	./target-common/x86_64-unknown-linux-musl/release/fledger --config "$(shell /usr/bin/mktemp -d -t fledger-XXXX)" --disable-turn-stun --signal-url ws://localhost:8765 realm create simulation --cond-pass

test_signal:
	./target-common/x86_64-unknown-linux-musl/release/flsignal -v --max-list-len 25

test_just_fetch_once:
	rm -rf /tmp/fledger
	./target-common/x86_64-unknown-linux-musl/release/fledger --config "$(shell /usr/bin/mktemp -d -t fledger-XXXX)" --disable-turn-stun --signal-url ws://localhost:8765 simulation just-fetch-once

clean:
	for c in ${CARGOS}; do \
		echo "Cleaning $$c"; \
		( cd $$c && cargo clean ) ; \
	done
