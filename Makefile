update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > common/src/node/version.rs

serve_local:
	pkill -f signal
	cargo build
	cargo run --bin signal &
	make -C web clean build_local serve &
	make -C cli/flnode run2
	open localhost:8080