CARGOS := cli/{fledger,flsignal} flarch flbrowser \
			flmodules flnet flnode test/{fledger-nodejs,signal-fledger,webrtc-libc-wasm/{libc,wasm}} \
			examples/ping-pong/{wasm,shared,libc}
MAKE_TESTS := test/{webrtc-libc-wasm,signal-fledger} examples/ping-pong
SHELL := /bin/bash

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

update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > flnode/src/version.rs

kill:
	pkill -f flsignal &
	pkill -f fledger &
	pkill -f "trunk serve" &

build_local_web:
	cd flbrowser && trunk build --features local

build_local_cli:
	cd cli && cargo build -p fledger && cargo build -p flsignal

build_local: build_local_web build_local_cli

serve_two: kill build_local_cli
	( cd cli && cargo run --bin flsignal -- -vv ) &
	sleep 4
	( cd cli && ( cargo run --bin fledger -- --config fledger/flnode -vv -s ws://localhost:8765 & \
		cargo run --bin fledger -- --config fledger/flnode2 -vv -s ws://localhost:8765 & ) )

serve_local: kill build_local_web serve_two
	cd flbrowser && trunk serve --features local &
	sleep 2
	open http://localhost:8080

docker_dev:
	rustup target add x86_64-unknown-linux-gnu
	mkdir -p cli/target/debug-x86
	docker run -ti -v $(PWD):/mnt/fledger:cached \
		rust:latest /mnt/fledger/run_docker.sh
	for cli in fledger flsignal; do \
		docker build cli/target/debug-x86 -f Dockerfile.$$cli -t fledgre/$$cli:dev && \
		# docker push fledgre/$$cli:dev; \
	done

clean:
	for c in ${CARGOS}; do \
		echo "Cleaning $$c"; \
		( cd $$c && cargo clean ) ; \
	done
