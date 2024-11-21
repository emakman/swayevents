.PHONY: build-debug build-release install

build-debug: target/debug/swayevents

target/debug/swayevents:
	cargo build

build-release: target/release/swayevents

target/release/swayevents:
	cargo build --release

install: build-release
	cargo install --path=. --target-dir=target
