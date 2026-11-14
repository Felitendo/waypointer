# waypointer: build and install
#
# Cargo builds the app, this Makefile installs it with the hands, the sounds, the icons and the
# desktop file. The binary looks for the hands and sounds in $(DATADIR)/waypointer, and in the
# checkout when it runs from there.

PREFIX   ?= /usr
DESTDIR  ?=
BINDIR   ?= $(PREFIX)/bin
DATADIR  ?= $(PREFIX)/share
CARGO    ?= cargo

HANDS    := $(wildcard assets/hand_*.png)
SOUNDS   := $(wildcard assets/sounds/tap_*.wav)

.PHONY: all build check test install uninstall clean run version

all: build

version:
	@$(CARGO) metadata --no-deps --format-version 1 | sed -n 's/.*"version":"\([^"]*\)".*/\1/p'

build:
	$(CARGO) build --release --locked

# Unit tests: the arm's geometry, the ink, every hand measured, the hand maker, the settings.
test:
	$(CARGO) test --locked

# Run after every change: tests, clippy, the desktop file, and no dashes in what users read.
check: test
	$(CARGO) clippy --locked -- -D warnings
	@if command -v desktop-file-validate >/dev/null 2>&1; then \
		desktop-file-validate res/de.felitendo.waypointer.desktop && echo "ok  desktop-file-validate"; \
	fi
	@if grep -rn "[—–]" README.md src res 2>/dev/null; then \
		echo "found a dash: rewrite the sentence" >&2; exit 1; \
	else \
		echo "ok  no dashes"; \
	fi

# Runs the build from the checkout. Quit an installed Waypointer first.
run:
	$(CARGO) run

install:
	install -Dm755 target/release/waypointer "$(DESTDIR)$(BINDIR)/waypointer"
	install -d "$(DESTDIR)$(DATADIR)/waypointer/hands" "$(DESTDIR)$(DATADIR)/waypointer/sounds"
	install -m644 -t "$(DESTDIR)$(DATADIR)/waypointer/hands" $(HANDS)
	install -m644 -t "$(DESTDIR)$(DATADIR)/waypointer/sounds" $(SOUNDS)
	install -Dm644 res/de.felitendo.waypointer.desktop "$(DESTDIR)$(DATADIR)/applications/de.felitendo.waypointer.desktop"
	cd res/icons && find hicolor -type f -exec install -Dm644 {} "$(DESTDIR)$(DATADIR)/icons/{}" \;
	install -Dm644 README.md "$(DESTDIR)$(DATADIR)/doc/waypointer/README.md"
	install -Dm644 LICENSE "$(DESTDIR)$(DATADIR)/licenses/waypointer/LICENSE"

uninstall:
	rm -f  "$(DESTDIR)$(BINDIR)/waypointer"
	rm -rf "$(DESTDIR)$(DATADIR)/waypointer"
	rm -f  "$(DESTDIR)$(DATADIR)/applications/de.felitendo.waypointer.desktop"
	cd res/icons && find hicolor -type f -exec rm -f "$(DESTDIR)$(DATADIR)/icons/{}" \;
	rm -rf "$(DESTDIR)$(DATADIR)/doc/waypointer" "$(DESTDIR)$(DATADIR)/licenses/waypointer"

clean:
	$(CARGO) clean
