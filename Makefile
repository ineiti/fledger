update_version:
	echo "pub const VERSION_STRING: &str = \"$$( date +%Y-%m-%d_%H:%M )::$$( git rev-parse --short HEAD )\";" > flnode/src/node/version.rs

kill_bg:
	pkill -f signal &
	pkill -f "npm run start" &

build_local:
	cargo build
	make -C web clean build_local 
	make -C cli/fledger build_local

serve_local: kill_bg build_local
	cargo run --bin signal &
	make -C web serve &
	make -C cli/fledger run2
	open localhost:8080
