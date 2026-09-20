# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities through [GitHub Security Advisories](https://github.com/tzhu01/PopSpeak/security/advisories/new).

**Do not open a public issue for security vulnerabilities.**

Your report should include:

- A descriptive title
- Severity assessment (Critical / High / Medium / Low)
- Affected component(s)
- Steps to reproduce
- Impact description

We will acknowledge your report within 72 hours and aim to release a fix within 14 days for critical issues.

## Security Model

PopSpeak is **local-first and offline by default**:

- Generic STT and LLM API-key fields use Windows Credential Manager
- Dedicated SeedASR credentials and custom cloud-vendor credentials are currently
  stored in plaintext in the local `settings.json`; protect that file and do not
  include it in support bundles or public repositories
- No account, cloud service or server-side storage is required for the core product
- Default SenseVoice/Whisper recognition keeps audio on the device
- Audio leaves the device only after the user explicitly selects a network provider
- Cloud proxy mode requires authentication via session token
- The application does not collect telemetry or usage data
- CSP is enabled in the Tauri webview

See [PRIVACY.md](PRIVACY.md) for the complete data-flow, local-storage and deletion
notes. Credential storage differs by settings field; do not assume that every
provider secret is protected by Windows Credential Manager.

## Out of Scope

The following are not considered vulnerabilities:

- Prompt injection in LLM responses (no security boundary to bypass)
- Users exposing their own API keys through misconfiguration
- Issues requiring physical access to the user's machine
- Vulnerabilities in third-party STT/LLM provider APIs
