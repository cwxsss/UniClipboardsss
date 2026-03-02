### ESLint 9 and Node.js Scripts

- In ESLint 9 (flat config), `/* global process */` might not be enough if the project doesn't include `globals.node` in the main config.
- For ESM scripts, importing `process` from `node:process` is a more robust way to satisfy `no-undef` rules.
- When using `import-x/order` with alphabetization, `node:` prefixed imports should be grouped and ordered correctly. Using `node:` prefix for all built-ins (`fs`, `os`, `path`, `url`) ensures consistency and satisfies linting rules.

### ESLint 9 and Node.js Scripts (Part 2)

- For ESM scripts in a project with strict linting, importing `process` from `node:process` is the most reliable way to fix `no-undef` errors for `process`.
- Using `node:` prefix for all built-in modules (`fs`, `os`, `path`, etc.) is recommended for consistency and to satisfy import ordering rules.
- Always verify that added imports are actually used to avoid `no-unused-vars` errors.

- Implemented update channel selector in AboutSection using existing UI patterns.
- Leveraged `UpdateChannel` type and `updateGeneralSetting` from existing hooks.
- Ensured immediate update check upon channel change for better UX.
