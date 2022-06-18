update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > shared/flnode/src/version.rs

kill_bg:
	pkill -f signal &
	pkill -f "npm run start" &

build_local:
	cargo build
	make -C web clean build_local 
	make -C cli/flnode build_local

serve_local: kill_bg build_local
	cargo run --bin signal &
	make -C web serve &
	make -C cli/flnode run2
	open localhost:8080

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
