---
description: Query Seq log server for application logs. Use "setup" as argument to configure API key.
user-invocable: true
---

# Seq Log Query

Query the Seq logging server to retrieve and analyze application logs.

## Configuration

- **Default URL**: `http://localhost:5341/api/events`
- **API Key file**: `.seq-api-key` (hidden file in project root, gitignored)

## Instructions

### Step 1: Check for "setup" mode

If `$ARGUMENTS` contains "setup":

1. Ask the user for their Seq API Key
2. Write the key to `.seq-api-key` in the project root (plain text, no newline)
3. Confirm the key has been saved
4. Stop here — do not query logs

### Step 2: Load API Key

1. Try to read `.seq-api-key` from the project root
2. If the file does not exist or is empty, inform the user:
   - "No Seq API key configured. Run `/seq setup` to configure it."
   - Stop here

### Step 3: Parse query arguments

Parse `$ARGUMENTS` to determine:

- **Filter/query**: What logs to search for (e.g., error messages, specific components, time ranges)
- **Custom URL**: If the user specifies a different Seq URL, use that instead of the default

### Step 4: Query Seq API

Use `curl` via Bash to query the Seq API:

- Base endpoint: `GET <url>/api/events`
- Include header: `X-Seq-ApiKey: <key from .seq-api-key>`
- Add query parameters as needed:
  - `?filter=<SeqQL filter>` for filtering logs
  - `?count=<number>` to limit results (default: 100)
  - `?signal=<signal>` for Seq signals

### Step 5: Present results

- Show timestamp, level, and message for each event
- Highlight errors and warnings
- Summarize patterns if there are many results

### Step 6: Handle errors

- **401/403**: API key may be invalid. Suggest running `/seq setup` to reconfigure
- **Connection refused**: Inform the user that Seq may not be running at the specified URL
- **Other errors**: Show the error details

## Example Usage

- `/seq setup` - Configure Seq API key
- `/seq` - Fetch recent 100 log events
- `/seq errors in the last hour` - Query for recent errors
- `/seq url=http://seq.example.com:5341` - Use a custom Seq URL
- `/seq filter="@Level = 'Error'"` - Use a specific SeqQL filter
