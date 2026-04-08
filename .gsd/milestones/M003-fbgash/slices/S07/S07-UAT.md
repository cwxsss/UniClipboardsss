# S07: Direct Daemon WS & Integration Proof Remediation — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T10:54:10.376Z

## UAT Test Cases (summary)

| # | Test | Pass Criterion |
|---|------|---------------|
| UAT-1 | Rust — query-param auth succeeds | 15/15 websocket_api tests pass |
| UAT-2 | Rust — header auth still works | websocket_api test passes |
| UAT-3 | Rust — envelope uses type/sessionId | 2/2 websocket_api tests pass |
| UAT-4 | Rust — pairing snapshots redact secrets | pairing_ws test passes |
| UAT-5 | Frontend — refreshSession before connect | "refreshSession before connect" test passes |
| UAT-6 | p2p-realtime contract uses daemonWs.subscribe | 6/6 p2p-realtime-contract tests pass |
| UAT-7 | Proof harness self-test | 5/5 checks pass, exit 0 |
| UAT-8 | Proof harness redaction | 0 raw tokens in output |
| UAT-9 | Consumer — useClipboardEventStream | 3/3 tests pass |
| UAT-10 | Docs exist | UAT runbook + security audit sections present |

See `.gsd/milestones/M003-fbgash/slices/S07/S07-UAT.md` for full UAT script with numbered steps and expected outputs.
