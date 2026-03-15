#!/usr/bin/env bash
# Seq Log Query Script
# Usage:
#   seq-query.sh [options]
#
# Options:
#   --url URL          Seq server URL (default: http://localhost:5341)
#   --filter FILTER    SeqQL filter expression
#   --signal SIGNAL    Seq signal name
#   --count N          Number of events to return (default: 100)
#   --level LEVEL      Filter by log level (Error, Warning, Information, Debug)
#   --from TIME        Start time (ISO 8601, e.g. 2024-01-01T00:00:00Z)
#   --to TIME          End time (ISO 8601)
#   --search TEXT      Full-text search in messages
#   --raw              Output raw JSON instead of formatted text

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
API_KEY_FILE="$PROJECT_ROOT/.seq-api-key"

# Defaults
SEQ_URL="http://localhost:5341"
FILTER=""
SIGNAL=""
COUNT=100
RAW=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --url)     SEQ_URL="$2"; shift 2 ;;
        --filter)  FILTER="$2"; shift 2 ;;
        --signal)  SIGNAL="$2"; shift 2 ;;
        --count)   COUNT="$2"; shift 2 ;;
        --level)   FILTER="@Level = '${2}'"; shift 2 ;;
        --from)    FROM_TIME="$2"; shift 2 ;;
        --to)      TO_TIME="$2"; shift 2 ;;
        --search)  SEARCH="$2"; shift 2 ;;
        --raw)     RAW=true; shift ;;
        --check-key)
            # Just check if API key exists
            if [[ -f "$API_KEY_FILE" ]] && [[ -s "$API_KEY_FILE" ]]; then
                echo "OK"
            else
                echo "MISSING"
            fi
            exit 0
            ;;
        --save-key)
            echo -n "$2" > "$API_KEY_FILE"
            echo "API key saved to $API_KEY_FILE"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Read API key
if [[ ! -f "$API_KEY_FILE" ]] || [[ ! -s "$API_KEY_FILE" ]]; then
    echo "ERROR: No Seq API key configured." >&2
    echo "Run '/seq setup' to configure it." >&2
    exit 2
fi
API_KEY="$(cat "$API_KEY_FILE")"

# Build query parameters
PARAMS="count=$COUNT"

# Build compound filter
FILTERS=()
if [[ -n "$FILTER" ]]; then
    FILTERS+=("$FILTER")
fi
if [[ -n "${FROM_TIME:-}" ]]; then
    FILTERS+=("@Timestamp >= DateTime('$FROM_TIME')")
fi
if [[ -n "${TO_TIME:-}" ]]; then
    FILTERS+=("@Timestamp <= DateTime('$TO_TIME')")
fi
if [[ -n "${SEARCH:-}" ]]; then
    FILTERS+=("@Message like '%$SEARCH%'")
fi

# Join filters with " and "
if [[ ${#FILTERS[@]} -gt 0 ]]; then
    COMBINED=""
    for i in "${!FILTERS[@]}"; do
        if [[ $i -gt 0 ]]; then
            COMBINED="$COMBINED and "
        fi
        COMBINED="$COMBINED${FILTERS[$i]}"
    done
    ENCODED_FILTER=$(python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.argv[1]))" "$COMBINED")
    PARAMS="$PARAMS&filter=$ENCODED_FILTER"
fi

if [[ -n "$SIGNAL" ]]; then
    ENCODED_SIGNAL=$(python3 -c "import urllib.parse, sys; print(urllib.parse.quote(sys.argv[1]))" "$SIGNAL")
    PARAMS="$PARAMS&signal=$ENCODED_SIGNAL"
fi

# Query Seq API
RESPONSE=$(curl -s -w "\n%{http_code}" \
    -H "X-Seq-ApiKey: $API_KEY" \
    "${SEQ_URL}/api/events?${PARAMS}" 2>&1) || {
    echo "ERROR: Failed to connect to Seq at $SEQ_URL" >&2
    echo "Seq may not be running or the URL is incorrect." >&2
    exit 3
}

# Split response body and HTTP status code
HTTP_CODE=$(echo "$RESPONSE" | tail -1)
BODY=$(echo "$RESPONSE" | sed '$d')

# Handle HTTP errors
case "$HTTP_CODE" in
    200) ;;
    401|403)
        echo "ERROR: Authentication failed (HTTP $HTTP_CODE)." >&2
        echo "API key may be invalid. Run '/seq setup' to reconfigure." >&2
        exit 4
        ;;
    *)
        echo "ERROR: Seq returned HTTP $HTTP_CODE" >&2
        echo "$BODY" >&2
        exit 5
        ;;
esac

# Output
if $RAW; then
    echo "$BODY"
else
    # Format output: extract timestamp, level, message from each event
    SEQ_BODY="$BODY" python3 <<'PYEOF'
import json, sys, os

try:
    data = json.loads(os.environ['SEQ_BODY'])
except json.JSONDecodeError as e:
    print(f'ERROR: Failed to parse response: {e}', file=sys.stderr)
    sys.exit(6)

events = data if isinstance(data, list) else data.get('Events', data.get('events', []))

if not events:
    print('No events found.')
    sys.exit(0)

# Level indicators
level_icons = {
    'Fatal': '[FATAL]',
    'Error': '[ERROR]',
    'Warning': '[WARN] ',
    'Information': '[INFO] ',
    'Debug': '[DEBUG]',
    'Verbose': '[TRACE]',
}

print(f'Found {len(events)} event(s):')
print('-' * 80)

for event in events:
    ts = event.get('Timestamp', event.get('timestamp', '?'))
    # Shorten timestamp
    if len(ts) > 19:
        ts = ts[:19].replace('T', ' ')

    level = event.get('Level', event.get('level', 'Information'))
    icon = level_icons.get(level, f'[{level}]')

    msg = event.get('RenderedMessage', event.get('renderedMessage',
          event.get('MessageTemplate', event.get('messageTemplate', ''))))
    if not msg:
        # Extract from MessageTemplateTokens
        tokens = event.get('MessageTemplateTokens', [])
        if tokens:
            msg = ''.join(t.get('Text', t.get('RawText', '')) for t in tokens)
        else:
            msg = '(no message)'

    exception = event.get('Exception', event.get('exception', ''))

    print(f'{ts}  {icon}  {msg}')
    if exception:
        # Print first 3 lines of exception
        exc_lines = exception.strip().split('\n')
        for line in exc_lines[:3]:
            print(f'    {line}')
        if len(exc_lines) > 3:
            print(f'    ... ({len(exc_lines) - 3} more lines)')

print('-' * 80)

# Summary
level_counts = {}
for event in events:
    level = event.get('Level', event.get('level', 'Information'))
    level_counts[level] = level_counts.get(level, 0) + 1

summary_parts = []
for level in ['Fatal', 'Error', 'Warning', 'Information', 'Debug', 'Verbose']:
    if level in level_counts:
        summary_parts.append(f'{level}: {level_counts[level]}')

print(f'Summary: {", ".join(summary_parts)}')
PYEOF
fi
