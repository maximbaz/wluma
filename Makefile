BIN := wluma
VERSION := 2.0.0

PREFIX ?= /usr
LIB_DIR = $(DESTDIR)$(PREFIX)/lib
BIN_DIR = $(DESTDIR)$(PREFIX)/bin
SHARE_DIR = $(DESTDIR)$(PREFIX)/share

.PHONY: run
run:
	cargo run

.PHONY: build
build:
	cargo build

.PHONY: clean
clean:
	rm -rf target dist

.PHONY: install
install:
	install -Dm755 -t "$(BIN_DIR)/" target/release/$(BIN)
	install -Dm644 -t "$(LIB_DIR)/systemd/user" "$(BIN).service"
	install -Dm644 -t "$(SHARE_DIR)/licenses/$(BIN)/" LICENSE
	install -Dm644 -t "$(SHARE_DIR)/doc/$(BIN)/" README.md

.PHONY: dist
dist: clean
	mkdir -p dist
	git archive -o "dist/$(BIN)-$(VERSION).tar.gz" --format tar.gz --prefix "$(BIN)-$(VERSION)/" "$(VERSION)"
	gpg --detach-sign --armor "dist/$(BIN)-$(VERSION).tar.gz"
	rm -f "dist/$(BIN)-$(VERSION).tar.gz"
