BIN := wluma
VERSION := 1.2.2

PREFIX ?= /usr
LIB_DIR = $(DESTDIR)$(PREFIX)/lib
BIN_DIR = $(DESTDIR)$(PREFIX)/bin
SHARE_DIR = $(DESTDIR)$(PREFIX)/share

.PHONY: run
run: build
	build/$(BIN)

.PHONY: build
build:
	meson build
	ninja -C build

.PHONY: clean
clean:
	rm -rf build dist

.PHONY: install
install:
	install -Dm755 -t "$(BIN_DIR)/" build/$(BIN)
	install -Dm644 -t "$(LIB_DIR)/systemd/user" "$(BIN).service"
	install -Dm644 -t "$(SHARE_DIR)/licenses/$(BIN)/" LICENSE
	install -Dm644 -t "$(SHARE_DIR)/doc/$(BIN)/" README.md

.PHONY: dist
dist: clean
	mkdir -p dist
	git archive -o "dist/$(BIN)-$(VERSION).tar.gz" --format tar.gz --prefix "$(BIN)-$(VERSION)/" "$(VERSION)"
	gpg --detach-sign --armor "dist/$(BIN)-$(VERSION).tar.gz"
	rm -f "dist/$(BIN)-$(VERSION).tar.gz"
