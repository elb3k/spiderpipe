BIN    := spiderpipe
PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin

.PHONY: all build release install uninstall clean help

all: release

help:
	@echo "make build      debug build      (target/debug/$(BIN))"
	@echo "make release    optimized build  (target/release/$(BIN))"
	@echo "make install    release build, copied to $(BINDIR)"
	@echo "make uninstall  remove $(BINDIR)/$(BIN)"
	@echo "make clean      remove build output"
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
