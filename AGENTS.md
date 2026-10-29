# AGENTS.md

Guidance for work on code in this repository.

## Build and linker

The Makefile exports `RUSTFLAGS` so every `cargo` call through `make` uses **mold**:

```makefile
RUSTFLAGS ?= -C link-arg=-fuse-ld=mold
export RUSTFLAGS
```

Use `make` targets when you want that linker. Bare `cargo` on the command line does not read the Makefile; export the same `RUSTFLAGS` yourself if you skip `make`.

On Arch and CachyOS, install the `mold` package if links fail with "cannot find -fuse-ld=mold". Override for one run with `make RUSTFLAGS='...'` if you need to.

**lld** (LLVM) is a common fallback. Use `RUSTFLAGS='-C link-arg=-fuse-ld=lld'` for a single build if mold is not available.

## Makefile targets

| Target | What it runs |
|--------|----------------|
| `make` / `make build` | Release `cargo build` for `faint-signal-cli` and `faint-signal-ui` |
| `make cli` | Release build, CLI only |
| `make ui` | Release build, UI only |
| `make debug` | Debug builds for both (`RELEASE=0`) |
| `make check` | `cargo check` on both packages |
| `make run` | `cargo run -p faint-signal-ui` |
| `make clean` | `cargo clean` |
| `make help` | Short target list |

`RELEASE=1` is the default. Pass `make RELEASE=0` or use `make debug` for debug artifacts.
