CARGOS := cli/{fledger,flsignal} flarch flbrowser \
			flmodules flnet flnode test/{fledger_node,webrtc-libc-wasm/{libc,wasm}} \
			examples/ping-pong/{wasm,shared,libc}

check:
	for c in ${CARGOS}; do \
	  echo Checking $$c; \
	  ( cd $$c && cargo check ); \
	done

update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > shared/flnode/src/version.rs

kill_bg:
	pkill -f flsignal &
	pkill -f fledger &
	pkill -f "npm run start" &

build_local_web:
	make -C flbrowser build_local 

build_local_cli:
	cd cli && cargo build

build_local: build_local_web build_local_cli

serve_two: kill_bg build_local_cli
	( cd cli && cargo run --bin flsignal -- -vv ) &
	sleep 4
	( cd cli && ( cargo run --bin fledger -- --config fledger/flnode -vv -s ws://localhost:8765 & \
		cargo run --bin fledger -- --config fledger/flnode2 -vv -s ws://localhost:8765 & ) )

serve_local: kill_bg build_local
	( cd cli/flsignal && cargo run -- -vv ) &
	sleep 4
	make -C flbrowser serve_local &
	( cd cli/fledger && ( cargo run -- -vv -s ws://localhost:8765 & \
		cargo run -- --config ./flnode2 -vv -s ws://localhost:8765 & ) )
	sleep 2
	open http://localhost:8080

update_crates:
	for cargo in $$( find . -name Cargo.toml ); do \
		echo $$cargo; \
		(cd $$(dirname $$cargo) && cargo update); \
	done

unused_crates:
	for cargo in $$( find . -name Cargo.toml ); do \
		(cd $$(dirname $$cargo) && cargo +nightly udeps -q ); \
	done

docker_dev:
	rustup target add x86_64-unknown-linux-gnu
	mkdir -p cli/target/debug-x86
	docker run -ti -v $(PWD):/mnt/fledger:cached \
		rust:latest /mnt/fledger/run_docker.sh
	for cli in fledger flsignal; do \
		docker build cli/target/debug-x86 -f Dockerfile.$$cli -t fledgre/$$cli:dev && \
		docker push fledgre/$$cli:dev; \
	done

clean:
	for c in ${CARGOS}; do \
		( cd $$c && cargo clean ) ; \
	done
