CARGOS := cli/{fledger,flsignal} flarch flarch_macro flbrowser \
			flmodules flnode test/{fledger-nodejs,signal-fledger,webrtc-libc-wasm/{libc,wasm}} \
			examples/ping-pong/{wasm,shared,libc}
MAKE_TESTS := test/{webrtc-libc-wasm,signal-fledger} examples/ping-pong
CRATES := flarch_macro flarch flmodules flnode
SHELL := /bin/bash
PKILL = @ps aux | grep "$1" | grep -v grep | grep -v Visual | awk '{print $$2}' | xargs -r kill

cargo_check:
	for c in ${CARGOS}; do \
	  echo Checking $$c; \
	  ( cd $$c && cargo check --tests ); \
	done

cargo_test:
	for c in ${CARGOS}; do \
	  echo Checking $$c; \
	  ( cd $$c && cargo test ); \
	done

make_test:
	for c in ${MAKE_TESTS}; do \
	  ( cd $$c && make test ); \
	done

cargo_update:
	for c in ${CARGOS}; do \
		echo Updating $$c; \
		(cd $$c && cargo update ); \
	done

cargo_clean:
	for c in ${CARGOS}; do \
		echo Cleaning $$c; \
		(cd $$c && cargo clean ); \
	done

cargo_build:
	for c in ${CARGOS}; do \
		echo Building $$c; \
		(cd $$c && cargo build ); \
	done

cargo_unused:
	for cargo in $$( find . -name Cargo.toml ); do \
		echo Checking for unused crates in $$cargo; \
		(cd $$(dirname $$cargo) && cargo +nightly udeps -q ); \
	done

publish:
	for crate in ${CRATES}; do \
		CRATE_VERSION=$$(cargo search $$crate | grep "^$$crate " | sed -e "s/.*= \"\(.*\)\".*/\1/"); \
		CARGO_VERSION=$$(grep "^version" $$crate/Cargo.toml | head -n 1 | sed -e "s/.*\"\(.*\)\".*/\1/"); \
		if [[ "$$CRATE_VERSION" != "$$CARGO_VERSION" ]]; then \
			echo "Publishing crate $$crate"; \
			cargo publish --token $$CARGO_REGISTRY_TOKEN --manifest-path $$crate/Cargo.toml; \
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
	( cd cli && cargo run --bin flsignal -- -vv ) &
	sleep 4
	( cd cli && ( cargo run --bin fledger -- --config fledger/flnode -vv -s ws://localhost:8765 & \
		cargo run --bin fledger -- --config fledger/flnode2 -vv -s ws://localhost:8765 & ) )

serve_local: kill build_local_web serve_two
	cd flbrowser && trunk serve --features local &
	sleep 2
	open http://localhost:8080

docker_dev:
	for cli in fledger flsignal; do \
		docker build --target $$cli --platform linux/amd64 -t fledgre/$$cli:dev . -f Dockerfile.dev --progress plain; \
		docker push fledgre/$$cli:dev; \
	done

clean:
	for c in ${CARGOS}; do \
		echo "Cleaning $$c"; \
		( cd $$c && cargo clean ) ; \
	done