update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > shared/flnode/src/node/version.rs

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