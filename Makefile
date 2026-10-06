.PHONY: check test build terminal sandbox linux-test
check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
test:
	cargo test --locked
build:
	cargo build --locked --release
terminal:
	cargo build --locked
	python3 tests/terminal.py target/debug/sweep
sandbox:
	./scripts/sandbox.sh
linux-test:
	./scripts/sandbox.sh test
