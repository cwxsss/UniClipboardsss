---
estimated_steps: 15
estimated_files: 1
skills_used: []
---

# T02: DaemonConfig and SessionToken types

Define TypeScript interfaces:

```typescript
interface DaemonConfig {
  baseUrl: string;   // e.g. "http://127.0.0.1:xxxxx"
  wsUrl: string;     // e.g. "ws://127.0.0.1:xxxxx/ws"
  pid: number;
  token: string;     // bearer token
}

interface SessionToken {
  token: string;     // JWT session token
  expiresAt: number; // unix timestamp ms
  encryptionReady: boolean;
}

function isSessionExpired(token: SessionToken | null): boolean
```

## Inputs

- None specified.

## Expected Output

- `src/api/daemon/types.ts`

## Verification

Types used correctly in DaemonClient. No runtime type errors.
