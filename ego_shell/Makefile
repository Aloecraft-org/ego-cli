TARGET_WASI:=--target wasm32-wasip2
TARGET_BROWSER:=--target wasm32-unknown-unknown
QUIET_WARN:=RUSTFLAGS="-Awarnings"

ifneq ($(filter quiet,$(MAKECMDGOALS)),)
CARGO_ENV:=$(QUIET_WARN)
else
CARGO_ENV:=
endif

quiet:
	@true

clean:
	cargo clean
	rm -rf target .data

_init:
	@mkdir -p .data/home .data/tmp


build_native:
	$(CARGO_ENV) cargo build
	$(CARGO_ENV) cargo build --bin cli_lite

build_native_tui:
	$(CARGO_ENV) cargo build --bin tui

build_wasi:
	$(CARGO_ENV) cargo build ${TARGET_WASI}
	$(CARGO_ENV) cargo build ${TARGET_WASI} --bin cli_lite

build_wasi_tui:
	$(CARGO_ENV) cargo build ${TARGET_WASI} --bin tui

build_web:
	$(CARGO_ENV) cargo build ${TARGET_BROWSER}


build_release:
	$(CARGO_ENV) cargo build --profile release
	$(CARGO_ENV) cargo build ${TARGET_WASI} --profile release
	$(CARGO_ENV) cargo build ${TARGET_BROWSER}  --profile release

check_native:
	$(CARGO_ENV)  cargo check

check_wasm:
	$(CARGO_ENV)  cargo check ${TARGET_WASI}

check_web:
	$(CARGO_ENV) cargo check ${TARGET_BROWSER}

check: check_native check_wasm check_web

run_native:
	$(CARGO_ENV) cargo run

run_wasi:
	$(CARGO_ENV) cargo run ${TARGET_WASI}

run_cli:
	$(CARGO_ENV) cargo run --bin cli_lite

run_cli_wasi:
	$(CARGO_ENV) cargo run ${TARGET_WASI} --bin cli_lite

run_tui:
	$(CARGO_ENV) RUSTFLAGS="-Awarnings"  cargo run -- --shell tui

run_web:
	@echo "http://127.0.0.1:9001/?mode=tui"
	@echo "http://127.0.0.1:9001/?mode=lite"

	trunk serve www/index.html --port 9001

build_tui_native:
	$(CARGO_ENV) cargo build --bin tui --release


test_wasi:
	$(CARGO_ENV) cargo test ${TARGET_WASI}
test_browser:
	$(CARGO_ENV) cargo test ${TARGET_BROWSER}
test_native:
	$(CARGO_ENV) cargo test
test: test_native test_wasi test_browser

