---
name: dual-side-debug
description: Inspect uniclipboard logs from BOTH the macOS host and the mounted Windows peer when debugging cross-platform sync, pairing, transfer, or daemon issues. Use whenever the user asks to "check logs", "see what's happening on both sides", or describes a symptom that involves the Windows peer (e.g. "Windows didn't receive...", "Mac sent but...", pairing/transfer/sync failures during dual-side dev).
user-invocable: true
---

# dual-side-debug

Inspect logs from the macOS host and the Windows peer in a single, time-aligned view.

This project is a Tauri desktop app where two peers (macOS + Windows) sync clipboard / files over an iroh-based network. The Windows machine's app data root is exposed to the Mac via SMB and mounted at `/tmp/win-uniclipboard/`, so both sides' JSONL logs are reachable from this host.

The helper script lives at `.claude/skills/dual-side-debug/dual-logs.sh`. It is the **only** thing you should need to invoke for log work — do not hand-roll `ls`/`tail`/`jq` pipelines unless the script can't express what you need.

## Log layout you must remember

* **macOS logs**: `~/Library/Application Support/app.uniclipboard.desktop[-<UC_PROFILE>]/logs/uniclipboard.json.YYYY-MM-DD`
* **Windows logs (mounted)**: `/tmp/win-uniclipboard/logs/uniclipboard.json.YYYY-MM-DD`
* Format: **JSON Lines**. Each line has at least `timestamp` (UTC, ISO-8601 with `Z`), `level`, `target`, `message`, `span`, `device_id`, plus structured fields.
* The date in the filename is **UTC**, not local time. A file named `...2026-04-25` can be the live file while it is still 2026-04-24 in PDT.

## Profile resolution (DO NOT skip this step)

The default macOS profile is **`dev`** (`package.json`'s `tauri:dev` script sets `UC_PROFILE=dev`). Always treat `dev` as the assumed profile **unless** the user has said otherwise in this conversation.

But the user often runs other profiles (`a`, `b` for `tauri:dev:peerA`/`peerB`, or ad-hoc names like `abc`). The active profile is a **runtime fact**, not a config you can read — you must verify it from disk.

Before answering any question that depends on log content, run:

```bash
.claude/skills/dual-side-debug/dual-logs.sh status
```

Then judge:

1. Does the assumed profile's log directory exist?
2. Is the latest log file's `mtime` close to "now" (look at the `freshness` column — `live (<2m)` or `recent (<10m)` is good; anything older means the process probably isn't running on this profile)?

**If the assumed profile dir is missing, OR the freshness is `stale` / `old` / `cold` while the user is actively reproducing**, stop and ask the user to confirm `UC_PROFILE`. The status output already lists the available profiles with their latest mtimes — use that to suggest a likely candidate. Example:

> dev profile dir doesn't exist on this machine. The most recently active profile is `abc` (last write 30s ago). Should I use `abc`, or are you running with a different `UC_PROFILE`?

Do not silently fall back. Wrong profile = looking at frozen logs from a previous session.

## Commands you'll use

Always invoke via the script. From the project root:

```bash
.claude/skills/dual-side-debug/dual-logs.sh <command> [args]
```

| Command         | When to use                                                                 |
| --------------- | --------------------------------------------------------------------------- |
| `status`        | First call of any debug session. Confirms profile + freshness on both sides.|
| `list-profiles` | When you suspect the user is on a profile other than `dev`.                 |
| `paths`         | Just need the resolved file paths (e.g. to feed another tool).              |
| `tail`          | Quick "what just happened" — defaults to last 50 lines, both sides.         |
| `grep <pat>`    | Plain string match. Cheap; good first probe (e.g. an error message, a device id). |
| `query --filter`| Structured `jq` filter against the JSONL. Best for level/target/span filters. |
| `merge`         | Time-interleave both sides into a single chronological stream. Use this whenever the question is *"what happened between Mac and Windows around time X"*. |

### Useful flags
* `--profile <name>` — override profile (default: `dev`).
* `--side mac|win|both` — restrict to one side.
* `--lines N` — output line cap.
* `--since <ISO8601>` (merge only) — drop lines older than this UTC timestamp.

## Recommended workflow

1. **Ground yourself.** Run `status`. Confirm both sides are live; resolve any profile mismatch with the user before continuing.
2. **Narrow the time window.** Ask the user when they reproduced the issue (or read it from their last message), convert to UTC, and pass it as `--since`.
3. **Start broad, then narrow.**
   * Broad: `merge --since <UTC> --lines 400` to see the cross-peer story.
   * Narrow: `query --filter '. | select(.level=="ERROR" or .level=="WARN")'` or filter by `target` (e.g. `iroh::magicsock`, `pairing`, `transfer`).
4. **Quote sparingly.** Logs are noisy. In your reply to the user, quote the 3–10 lines that actually carry signal, with the side prefix and timestamp. Don't dump raw JSONL walls.
5. **Cross-reference, don't assume.** If the symptom is "Mac says sent, Windows didn't receive", verify by *grepping the same id (transfer id, blob hash, request id) on both sides*. The merged view is much stronger than two parallel monologues.

## jq filter cookbook

These plug straight into `query --filter '<jq>'`:

```jq
# Errors and warnings only
. | select(.level == "ERROR" or .level == "WARN")

# Restrict to one subsystem (substring match on target)
. | select(.target | test("pairing|setup|transfer"))

# A specific span chain
. | select(.span | test("handle_pong"))

# Around a particular device id
. | select(.device_id == "47a545ac-6d31-413c-b9fe-315ee4be0fb0")

# Compact projection for human reading
. | {ts: .timestamp, lvl: .level, tgt: .target, msg: .message, span}
```

For the merged view (script already injects `.side`):

```jq
{ts: .timestamp, side: .side, lvl: .level, tgt: .target, msg: .message}
```

## Things to avoid

* Don't `cat` whole log files — they're hundreds of MB.
* Don't infer "Windows is broken" without first checking the Windows log freshness; the SMB mount can lag, and a stale `mtime` may just mean the Windows app is paused.
* Don't translate UTC ↔ local time in your head and silently. If you do convert, say so (e.g. "logs around 17:30 PDT = 00:30 UTC the next day").
* Don't add or modify Mac log paths in this skill if the layout in `src-tauri/AGENTS.md` changes — fix `dual-logs.sh` first, then this doc.

## When this skill does *not* apply

* User wants to query the **Seq** server (the project also ships logs there) — use the `seq` skill instead.
* User is debugging build / cargo / typecheck failures — those don't go through these JSONL logs.
* User asks about the daemon HTTP API or sqlite state — those are separate; logs are observability, not state.
