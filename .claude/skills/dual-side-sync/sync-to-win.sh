#!/usr/bin/env bash
# sync-to-win.sh — Push the macOS working-tree changes to the mounted Windows repo.
#
# Mac repo (source):  $MAC_REPO   (default: /Volumes/ExternalSSD/superset/uniclipboard/slender-soybean)
# Win repo (target):  $WIN_REPO   (default: /tmp/uniclipboard-win, SMB mount of \\DESKTOP-HIC7MLI\Users\mark\projects\UniClipboard)
#
# Strategy:
#   1. Require both sides at the same git HEAD (otherwise we'd silently overwrite a different commit).
#   2. Require Win working tree clean (or pass --force to reset it first).
#   3. Apply tracked changes via `git diff HEAD --binary | git apply` on the Win side.
#   4. Copy untracked-but-not-ignored files via cp.
#
# We never touch .git/ on the Win side, never modify Win HEAD, and never push commits.
# This is a working-tree mirror — the Win repo stays "owned" by git, you just borrow its working tree.

set -euo pipefail

MAC_REPO="${MAC_REPO:-/Volumes/ExternalSSD/superset/uniclipboard/slender-soybean}"
WIN_REPO="${WIN_REPO:-/tmp/uniclipboard-win}"

usage() {
  cat <<'EOF'
Usage: sync-to-win.sh <command> [options]

Commands:
  status            Show both sides: path, branch, HEAD, dirty files. Always run first.
  diff              Show what would be synced from mac (tracked diffstat + untracked list).
  push              Apply mac working-tree changes to win.
                    Refuses if HEADs differ or win is dirty.
        --force     Auto-reset win first if it is dirty (drops any prior local edits on win).
        --dry-run   Print the plan without modifying the win repo.
  reset             Reset win working tree to HEAD and clean untracked files
                    (preserves target/ and node_modules/ to avoid a full rebuild).
  paths             Print MAC_REPO / WIN_REPO.

Env overrides:
  MAC_REPO   Source repo on this Mac.
  WIN_REPO   Mounted Windows repo path.
EOF
}

require_dirs() {
  # `.git` may be a file (git worktree pointer), so use rev-parse instead of `-d`.
  [[ -e "$MAC_REPO/.git" ]] && git -C "$MAC_REPO" rev-parse --git-dir >/dev/null 2>&1 \
    || { echo "✗ MAC_REPO is not a git repo: $MAC_REPO" >&2; exit 1; }
  [[ -e "$WIN_REPO/.git" ]] && git -C "$WIN_REPO" rev-parse --git-dir >/dev/null 2>&1 \
    || { echo "✗ WIN_REPO is not a git repo (mount may be down): $WIN_REPO" >&2; exit 1; }
}

g_mac() { git -C "$MAC_REPO" "$@"; }
g_win() { git -C "$WIN_REPO" "$@"; }

# --- commands --------------------------------------------------------------

cmd_status() {
  require_dirs
  local mh wh mb wb md wd
  mh=$(g_mac rev-parse HEAD)
  wh=$(g_win rev-parse HEAD)
  mb=$(g_mac branch --show-current)
  wb=$(g_win branch --show-current)
  md=$(g_mac status --short)
  wd=$(g_win status --short)

  echo "=== mac (source) ==="
  echo "  path:   $MAC_REPO"
  echo "  branch: $mb"
  echo "  HEAD:   $mh"
  if [[ -n "$md" ]]; then
    echo "  dirty:"
    printf '%s\n' "$md" | sed 's/^/    /'
  else
    echo "  dirty:  (clean)"
  fi
  echo
  echo "=== win (target) ==="
  echo "  path:   $WIN_REPO"
  echo "  branch: $wb"
  echo "  HEAD:   $wh"
  if [[ -n "$wd" ]]; then
    echo "  dirty:"
    printf '%s\n' "$wd" | sed 's/^/    /'
  else
    echo "  dirty:  (clean)"
  fi
  echo
  if [[ "$mh" == "$wh" ]]; then
    echo "✓ HEAD aligned"
  else
    echo "✗ HEAD mismatch — sync refuses until aligned (git push/pull both repos to the same commit)"
  fi
  if [[ "$mb" != "$wb" ]]; then
    echo "! branch differs (mac=$mb, win=$wb) — usually fine but worth noting"
  fi
}

cmd_diff() {
  require_dirs
  echo "--- tracked changes (mac vs HEAD) ---"
  g_mac diff HEAD --stat || true
  echo
  echo "--- untracked files (would be copied) ---"
  local untracked
  untracked=$(g_mac ls-files --others --exclude-standard)
  if [[ -n "$untracked" ]]; then
    printf '%s\n' "$untracked"
  else
    echo "(none)"
  fi
}

cmd_push() {
  require_dirs
  local force=0 dry=0
  for a in "$@"; do
    case "$a" in
      --force)   force=1;;
      --dry-run) dry=1;;
      *) echo "unknown arg: $a" >&2; exit 2;;
    esac
  done

  local mh wh
  mh=$(g_mac rev-parse HEAD)
  wh=$(g_win rev-parse HEAD)
  if [[ "$mh" != "$wh" ]]; then
    echo "✗ HEAD mismatch (mac=$mh win=$wh). Align both repos at the same commit first." >&2
    exit 1
  fi

  local win_dirty
  win_dirty=$(g_win status --porcelain)
  if [[ -n "$win_dirty" ]]; then
    if [[ $force -eq 0 ]]; then
      echo "✗ win working tree not clean. Re-run with --force to reset, or use 'reset' first." >&2
      printf '%s\n' "$win_dirty" | head -10 | sed 's/^/    /' >&2
      exit 1
    fi
    if [[ $dry -eq 1 ]]; then
      echo "[dry-run] would: git -C $WIN_REPO reset --hard HEAD && git clean -fd -- :!target :!node_modules"
    else
      echo "→ resetting win working tree…"
      g_win reset --hard HEAD >/dev/null
      g_win clean -fd -- ':!target' ':!node_modules' >/dev/null || true
    fi
  fi

  # Tracked changes via patch.
  # Bake the path into the trap string so we don't depend on a local var at EXIT (set -u).
  local patchfile
  patchfile=$(mktemp -t uniclipboard-sync.XXXXXX.patch)
  trap "rm -f '$patchfile'" EXIT
  g_mac diff HEAD --binary > "$patchfile"

  local file_count=0
  if [[ -s "$patchfile" ]]; then
    file_count=$(grep -c '^diff --git' "$patchfile" || true)
    if [[ $dry -eq 1 ]]; then
      echo "[dry-run] would apply $file_count tracked-file diff to win"
    else
      echo "→ applying tracked diff to win ($file_count files)…"
      if ! g_win apply --check "$patchfile" 2>/dev/null; then
        echo "  patch did not apply cleanly; retrying with --3way…"
        if ! g_win apply --3way "$patchfile"; then
          # Preserve patch for inspection — disarm trap and copy out to /tmp.
          local saved="/tmp/uniclipboard-sync.failed.patch"
          cp "$patchfile" "$saved"
          trap - EXIT
          rm -f "$patchfile"
          echo "✗ git apply failed even with --3way. Patch saved at: $saved" >&2
          exit 1
        fi
      else
        g_win apply "$patchfile"
      fi
      echo "  ✓ applied"
    fi
  else
    echo "→ no tracked changes to apply"
  fi

  # Untracked files
  local untracked
  untracked=$(g_mac ls-files --others --exclude-standard)
  if [[ -n "$untracked" ]]; then
    local n
    n=$(printf '%s\n' "$untracked" | wc -l | tr -d ' ')
    if [[ $dry -eq 1 ]]; then
      echo "[dry-run] would copy $n untracked files:"
      printf '%s\n' "$untracked" | sed 's/^/    + /'
    else
      echo "→ copying $n untracked files…"
      while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        mkdir -p "$WIN_REPO/$(dirname "$f")"
        cp -p "$MAC_REPO/$f" "$WIN_REPO/$f"
        echo "    + $f"
      done <<< "$untracked"
    fi
  fi

  if [[ $dry -eq 0 ]]; then
    echo
    echo "✓ done. win status:"
    g_win status --short | head -20 | sed 's/^/    /'
  fi
}

cmd_reset() {
  require_dirs
  echo "→ git reset --hard HEAD on win"
  g_win reset --hard HEAD >/dev/null
  echo "→ git clean -fd on win (preserving target/ node_modules/)"
  g_win clean -fd -- ':!target' ':!node_modules' >/dev/null || true
  echo "✓ win is clean"
  g_win status --short | head -10 | sed 's/^/    /'
}

cmd_paths() {
  echo "MAC_REPO=$MAC_REPO"
  echo "WIN_REPO=$WIN_REPO"
}

# --- dispatch --------------------------------------------------------------

cmd="${1:-}"; shift || true
case "$cmd" in
  status) cmd_status "$@";;
  diff)   cmd_diff "$@";;
  push)   cmd_push "$@";;
  reset)  cmd_reset "$@";;
  paths)  cmd_paths "$@";;
  -h|--help|help|"") usage;;
  *) echo "unknown command: $cmd" >&2; usage; exit 2;;
esac
