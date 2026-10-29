# FaintSignal

Linux Signal linked-device client. Rust, no Electron. Low idle RAM.

Official Signal Desktop is a slow vampire. Leave a chat open and it keeps a fat Chromium heap around for that one job. I do not want a gigabyte of RAM reserved so a messenger can sit idle. FaintSignal is the attempt to talk Signal without that tax.

It uses official `libsignal` through [presage](https://github.com/whisperfish/presage). Voice calls use Signal [RingRTC](https://github.com/signalapp/ringrtc) when you build that feature. Video calls stay out of v1.

## AI usage

I barely know Rust. This exists because I need the tool, not because I set out to learn the language. Cursor Auto writes most of the code. I set direction and review what it produces.

## License

FaintSignal is **AGPL-3.0-only**. That matches the Signal libraries it links (`libsignal`, `presage`, `libsignal-service-rs`, and RingRTC if enabled). See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## Status

**Work in progress.** This is not ready for daily use. Linking, basic send/receive, and a thin UI exist. Contacts sync, groups, attachments, receipts, edits, and solid calling still need a lot of work. Expect breakage. Treat it as a lab project, not a messenger you depend on.

This is an unofficial client. Signal does not support it. Protocol fields move. Pin library revisions and read `docs/protocol-compat.md`.

## Build

The Makefile exports `RUSTFLAGS='-C link-arg=-fuse-ld=mold'`, so the targets below invoke `cargo` with **mold** as the linker. Prefer `make` over bare `cargo` if you want that default. **lld** from LLVM works too if you set `RUSTFLAGS='-C link-arg=-fuse-ld=lld'` for a run.

```bash
# system deps (Arch / CachyOS example)
sudo pacman -S base-devel openssl sqlcipher cmake protobuf clang mold

make          # release cli + ui (same as make build)
make build
make cli      # release CLI only
make ui       # release UI only
make debug    # debug cli + ui
make check    # cargo check both packages
make run      # run the iced UI
make clean
make help
```

Same builds without `make` (set linker yourself):

```bash
export RUSTFLAGS='-C link-arg=-fuse-ld=mold'
cargo build -p faint-signal-cli --release
cargo build -p faint-signal-ui --release
```

First builds compile `libsignal` and take a long time.

## CLI

```bash
# show data paths and RSS
cargo run -p faint-signal-cli -- paths

# link as secondary device (scan QR with the phone)
cargo run -p faint-signal-cli -- link-device --device-name FaintSignal

# sync inbox once
cargo run -p faint-signal-cli -- receive

# stay connected
cargo run -p faint-signal-cli -- receive --watch

# send text (ACI UUID)
cargo run -p faint-signal-cli -- send <ACI-UUID> "hello"

# voice call (needs RingRTC — see docs/calling.md)
cargo run -p faint-signal-cli -- call <ACI-UUID>
```

Optional DB passphrase: `FAINTSIGNAL_PASSPHRASE` or `--passphrase`.

## GUI

```bash
make run      # mold via Makefile RUSTFLAGS
# or: cargo run -p faint-signal-ui   # export RUSTFLAGS yourself for mold
```

Link from the UI button. The QR prints on the terminal that started the process.

## Layout

| Crate | Role |
|---|---|
| `faint-signal-core` | Presage session, send/receive, call hand-off |
| `faint-signal-store` | App SQLite cache (paged messages) |
| `faint-signal-calls` | Voice call state machine + media engine trait |
| `faint-signal-cli` | `faintsignal` binary |
| `faint-signal-ui` | iced Linux UI |

## Docs

- `docs/memory-budget.md` — RSS targets
- `docs/protocol-compat.md` — feature matrix vs Desktop
- `docs/calling.md` — RingRTC Linux build and signaling

## Memory goal

Idle RSS under 100 MB on Linux after link. Measure with `faintsignal rss` or `paths`.
