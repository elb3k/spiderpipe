BIN    := spiderpipe
PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin

.PHONY: all build release install uninstall clean help tag-release

all: release

help:
	@echo "make build      debug build      (target/debug/$(BIN))"
	@echo "make release    optimized build  (target/release/$(BIN))"
	@echo "make install    release build, copied to $(BINDIR)"
	@echo "make uninstall  remove $(BINDIR)/$(BIN)"
	@echo "make clean      remove build output"
	@echo "make tag-release VERSION=x.y.z"
	@echo "                bump version, commit, tag and push (CI then updates the formula)"
	@echo
	@echo "Install elsewhere: sudo make install PREFIX=/usr/local"

build:
	cargo build

release:
	cargo build --release

install: release
	install -d "$(BINDIR)"
	install -m 755 target/release/$(BIN) "$(BINDIR)/$(BIN)"
	@echo "installed $(BINDIR)/$(BIN)"

uninstall:
	rm -f "$(BINDIR)/$(BIN)"

clean:
	cargo clean

tag-release:
	@test -n "$(VERSION)" || { echo "usage: make tag-release VERSION=x.y.z"; exit 1; }
	@test "$$(git branch --show-current)" = main || { echo "switch to main first"; exit 1; }
	@git diff --quiet && git diff --cached --quiet || { echo "commit or stash your changes first"; exit 1; }
	@! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null || { echo "tag v$(VERSION) already exists"; exit 1; }
	git pull --ff-only origin main
	perl -pi -e 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml
	cargo build
	git commit -am "Release v$(VERSION)"
	git tag "v$(VERSION)"
	git push origin main "v$(VERSION)"
	@echo "pushed v$(VERSION); GitHub Actions will update the formula. Run 'git pull' in a minute."
