---
estimated_steps: 19
estimated_files: 1
skills_used: []
---

# T03: DaemonApiError class with typed error codes

Create `src/api/daemon/errors.ts`:

```typescript
export class DaemonApiError extends Error {
  code: DaemonErrorCode;
  message: string;
  details?: unknown;
  constructor(code: DaemonErrorCode, message: string, details?: unknown)
}

export enum DaemonErrorCode {
  UNAUTHORIZED = 'UNAUTHORIZED',
  FORBIDDEN = 'FORBIDDEN',
  NOT_FOUND = 'NOT_FOUND',
  RATE_LIMITED = 'RATE_LIMITED',
  ENCRYPTION_NOT_READY = 'ENCRYPTION_NOT_READY',
  CONFIRMATION_REQUIRED = 'CONFIRMATION_REQUIRED',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
}
```

Map HTTP status codes to error codes: 401→UNAUTHORIZED, 403→FORBIDDEN, 404→NOT_FOUND, 429→RATE_LIMITED, 503→ENCRYPTION_NOT_READY (or parse from response body).

## Inputs

- None specified.

## Expected Output

- `src/api/daemon/errors.ts`

## Verification

Unit tests: error thrown correctly for each HTTP status, code and message fields populated from response.
