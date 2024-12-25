BIN := wluma
VERSION := 4.6.0

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
	cargo test --locked

.PHONY: lint
lint:
	cargo fmt -- --check
	cargo clippy -- -Dwarnings

.PHONY: run
run:
	cargo run

.PHONY: clean
clean:
	rm -rf dist

.PHONY: docs
docs:
	marked-man -i README.md -o "$(BIN).7"
	gzip "$(BIN).7"

.PHONY: install
install:
	install -Dm755 -t "$(BIN_DIR)/" "target/release/$(BIN)"
	install -Dm644 -t "$(LIB_DIR)/udev/rules.d/" "90-$(BIN)-backlight.rules"
	install -Dm644 -t "$(LIB_DIR)/systemd/user" "$(BIN).service"
	install -Dm644 -t "$(SHARE_DIR)/licenses/$(BIN)/" LICENSE
	install -Dm644 -t "$(SHARE_DIR)/doc/$(BIN)/" README.md
	install -Dm644 -t "$(SHARE_DIR)/man/man7" "$(BIN).7.gz"
	install -Dm644 -t "$(SHARE_DIR)/$(BIN)/examples/" config.toml

.PHONY: dist
dist: clean build
	mkdir -p dist
	cp "target/release/$(BIN)" .
	tar -czvf "dist/$(BIN)-$(VERSION)-linux-x86_64.tar.gz" "$(BIN)" "90-$(BIN)-backlight.rules" "$(BIN).service" LICENSE README.md config.toml Makefile
	git -c tar.tar.gz.command="gzip -cn" archive -o "dist/$(BIN)-$(VERSION).tar.gz" --format tar.gz --prefix "$(BIN)-$(VERSION)/" "$(VERSION)"
	for f in dist/*.tar.gz; do gpg --detach-sign --armor "$$f"; done
	rm -f "dist/$(BIN)-$(VERSION).tar.gz" "$(BIN)"
