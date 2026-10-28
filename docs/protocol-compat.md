# Protocol compatibility matrix

Unofficial linked-device client. Compare against official Signal Desktop messaging and 1:1 voice.

Legend: `done` = implemented path exists, `partial` = structure only, `todo` = not started, `hold` = out of v1 on purpose.

| Feature | Status | Notes |
|---|---|---|
| QR link secondary device | done | `faintsignal link-device` via presage |
| whoami / registered load | done | |
| Receive 1:1 text | done | `receive` / `--watch` |
| Send 1:1 text | done | `send <aci> <text>` |
| App message cache (paged) | done | `faint-signal-store` |
| Group receive / send | partial | receive caches groups; `send-group` CLI for master-key hex |
| Attachments download | partial | `get_attachment` on receive; files under `attachments/` |
| Reactions / edits / deletes | todo | |
| Typing / receipts | todo | |
| Disappearing messages | todo | |
| Safety Number UX | todo | |
| Sealed sender | partial | handled inside libsignal/presage |
| Profiles / contacts sync | partial | `Received::Contacts` logged |
| 1:1 voice signaling parse | partial | `CallMessage` → `CallSignal` |
| 1:1 voice media (RingRTC) | partial | `NullMediaEngine`; enable `ringrtc` feature after `docs/calling.md` |
| Voice mute / hangup state | done | state machine in `faint-signal-calls` |
| Video calls | hold | Phase 5 |
| Group calling | hold | Phase 5 |
| Stories / payments | hold | |
| Primary device register | hold | phone stays primary |

## Library pins

| Dependency | Pin | Bump process |
|---|---|---|
| presage | git rev in root `Cargo.toml` (gurk-known) | After Whisperfish/gurk move; run CLI link + receive smoke |
| libsignal-service | via that presage rev | same |
| RingRTC | documented in `calling.md` | Match Desktop release notes |

## Fail closed

Unknown or video call offers return errors. Do not write bad session state. Prefer a loud error over a silent decrypt break.
