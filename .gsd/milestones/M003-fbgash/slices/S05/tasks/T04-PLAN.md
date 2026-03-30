---
estimated_steps: 11
estimated_files: 1
skills_used: []
---

# T04: Security audit and token leakage check

Security audit checklist:

1. **Token leakage**: grep source for localStorage.setItem('token'), sessionStorage.setItem('token'), document.cookie with token → should find zero
2. **Bearer token placement**: verify Authorization header set only, never in URL query params
3. **Rate limiting**: send 101 requests in <1 minute → 429 on 101st
4. **Permission enforcement**:
   - L2 (no auth): health check works without session
   - L3 without encryption session: call encryption-modifying endpoint → ENCRYPTION_NOT_READY error
   - L4 without confirmation: call clear-cache with confirmed:false → 400 CONFIRMATION_REQUIRED
5. **PID verification**: make request from process with wrong PID → rejection
6. **CORS**: daemon HTTP responses should not have Access-Control-Allow-Origin: * (localhost is fine, but verify no wildcard)

Document findings in a security audit report.

## Inputs

- `src/api/daemon/client.ts`
- `src-tauri/crates/uc-daemon/src/api/ (middleware)`

## Expected Output

- `docs/security-audit.md`

## Verification

All security checks pass. Audit report documents each check and result. No critical issues found.
