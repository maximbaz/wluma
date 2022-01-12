BIN := wluma
VERSION := 2.0.1

PREFIX ?= /usr
LIB_DIR = $(DESTDIR)$(PREFIX)/lib
BIN_DIR = $(DESTDIR)$(PREFIX)/bin
SHARE_DIR = $(DESTDIR)$(PREFIX)/share

.PHONY: build-dev
build-dev:
	cargo build

.PHONY: build
build:
	cargo build --locked --release

.PHONY: test
test:
	cargo test --workspace --locked

.PHONY: lint
lint:
	cargo fmt -- --check
	RUSTFLAGS="-Dwarnings" cargo clippy --workspace -- -D warnings

.PHONY: run
run:
	cargo run

.PHONY: clean
clean:
	rm -rf dist

.PHONY: install
install:
	install -Dm755 -t "$(BIN_DIR)/" "target/release/$(BIN)"
	install -Dm644 -t "$(LIB_DIR)/udev/rules.d/" "90-$(BIN)-backlight.rules"
	install -Dm644 -t "$(LIB_DIR)/systemd/user" "$(BIN).service"
	install -Dm644 -t "$(SHARE_DIR)/licenses/$(BIN)/" LICENSE
	install -Dm644 -t "$(SHARE_DIR)/doc/$(BIN)/" README.md
	install -Dm644 -t "$(SHARE_DIR)/$(BIN)/examples/" config.toml

.PHONY: dist
dist: clean build
	mkdir -p dist
	tar -czvf "dist/$(BIN)-$(VERSION)-linux-x86_64.tar.gz" "target/release/$(BIN)" 90-$(BIN)-backlight.rules "$(BIN).service" LICENSE README.md config.toml Makefile
	git archive -o "dist/$(BIN)-$(VERSION).tar.gz" --format tar.gz --prefix "$(BIN)-$(VERSION)/" "$(VERSION)"
	for f in dist/*.tar.gz; do gpg --detach-sign --armor "$$f"; done
	rm -f "dist/$(BIN)-$(VERSION).tar.gz"
