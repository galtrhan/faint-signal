# Memory budget

## Targets

| State | Target RSS | How to measure |
|---|---|---|
| Idle, linked, no UI scroll | under 100 MB | `faintsignal rss` after `receive` |
| Stretch idle (CLI only) | 30–60 MB | same |
| One open chat, ~1k messages paged | under 150 MB | UI scroll test |
| Active 1:1 voice call | measure separately | during call; WebRTC will raise RSS |

Electron Signal Desktop often sits in the hundreds of MB idle. FaintSignal should stay well below that.

## Rules in code

1. Page messages in `faint-signal-store`. Do not load full history into a `Vec`.
2. Keep attachments on disk under `attachments/`. Decode thumbnails with an LRU cap (16–32 MB) when the UI adds them.
3. One messaging websocket. Back off on reconnect.
4. Do not start RingRTC until a call starts. Tear it down on hangup.

## Commands

```bash
cargo run -p faint-signal-cli --release -- paths
cargo run -p faint-signal-cli --release -- rss
```

`print_rss` reads `VmRSS` from `/proc/self/status`.

## Measured (Phase 0 spike, this machine)

| Binary | State | RSS |
|---|---|---|
| `faintsignal` release | `paths` / `rss` (not linked) | ~6.6–6.8 MB |

Linked + receive will be higher once the Signal store and websocket are open. Re-measure after `link-device` and update this table.

## Pin note

Presage and libsignal dominate binary size and warm RSS. Prefer release builds with `lto = "thin"` (workspace default).
