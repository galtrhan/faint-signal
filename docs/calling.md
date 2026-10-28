# Calling (1:1 voice)

FaintSignal v1 requires **audio-only 1:1** calls. Video and group calls stay on hold.

## Stack

Signal clients use [RingRTC](https://github.com/signalapp/ringrtc) on WebRTC. Call setup messages ride the normal encrypted Signal channel (`CallMessage`). FaintSignal mirrors that split:

1. `faint-signal-core` decrypts inbound content and maps `CallMessage` to `CallSignal`.
2. `faint-signal-calls::CallManager` owns offer / answer / hangup / mute state.
3. `MediaEngine` runs the media path. Default is `NullMediaEngine` (compiles without WebRTC).

Whisperfish still lists voice calls as unchecked. Expect Desktop/Android signaling reverse-engineering when you wire RingRTC.

## Build RingRTC on Linux

1. Clone RingRTC and follow its README build for Linux (WebRTC fetch is large).
2. Produce a linkable library / Rust crate artifact your toolchain can use.
3. Implement `MediaEngine` in `crates/faint-signal-calls` behind feature `ringrtc`.
4. Build FaintSignal with `--features faint-signal-calls/ringrtc` once the impl exists.
5. Capture/playback: PipeWire (Pulse compat is fine). No camera in v1.

Do not invent a custom VoIP stack. Official phones and Desktop will not interoperate with one.

## Lifecycle (memory)

1. Create the RingRTC call object on dial or accept.
2. Destroy it on hangup.
3. Measure RSS idle vs in-call (`docs/memory-budget.md`).

## Interop checklist

- FaintSignal → phone voice
- Phone → FaintSignal voice
- FaintSignal ↔ official Desktop
- Busy when a second call arrives
- Mute round-trip

## Current CLI

```bash
cargo run -p faint-signal-cli -- call <ACI-UUID>
```

Without RingRTC this returns `RingRtcUnavailable`. The state machine and UI call chrome are already present.
