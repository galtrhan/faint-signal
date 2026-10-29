# FaintSignal — release builds for CLI and iced UI.
CARGO ?= cargo
# Empty for debug. Set RELEASE=1 (default) for --release.
RELEASE ?= 1
ifeq ($(RELEASE),1)
CARGO_FLAGS := --release
else
CARGO_FLAGS :=
endif

.PHONY: all build cli ui run debug check clean help

all: build

build: cli ui

cli:
	$(CARGO) build -p faint-signal-cli $(CARGO_FLAGS)

ui:
	$(CARGO) build -p faint-signal-ui $(CARGO_FLAGS)

run:
	$(CARGO) run -p faint-signal-ui

debug:
	$(MAKE) build RELEASE=0

check:
	$(CARGO) check -p faint-signal-cli
	$(CARGO) check -p faint-signal-ui

clean:
	$(CARGO) clean

help:
	@echo "Targets: all/build (release cli+ui), cli, ui, run, debug, check, clean"
	@echo "Run UI:  make run"
	@echo "Debug:   make debug   or   make RELEASE=0"
