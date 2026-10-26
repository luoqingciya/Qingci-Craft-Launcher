# Security Policy

[中文](SECURITY.md) | **English**

## Supported versions

The project is in early development (`0.x`). Security fixes are provided for the latest release only.

## Reporting a vulnerability

**Please do not report security issues through public issues.**

Use GitHub's private vulnerability reporting:
<https://github.com/luoqingciya/Qingci-Craft-Launcher/security/advisories/new>

Please include: affected version, reproduction steps, impact assessment and, if possible, a suggested fix.

We will acknowledge within 7 days and provide a remediation plan within 30 days of confirmation.

## Security boundaries

The launcher ultimately handles third-party credentials and executable downloads. Below is what is
**implemented today**, followed by what is **not yet implemented** — treat the latter as current
risk, not as an empty promise on a wishlist.

### Implemented

1. **Download integrity**: every file fetched from Mojang endpoints is verified against the official
   SHA-1. Files with a missing or mismatching digest are never used.
2. **Path traversal protection**: extracting assets and natives rejects `..`, leading `/`, drive
   letters and backslashes, so nothing is written outside the game directory.
3. **No unverified execution**: the game is launched only from official artifacts; loader (Fabric)
   libraries are verified the same way.
4. **No tokens in logs**: the auth layer (`qcl-auth`) never prints tokens and never persists them.

### Not yet implemented (current risk)

1. **No credential storage.** Only offline accounts are supported right now, so there are no tokens
   to store. The Microsoft login protocol layer is complete but **not wired into the UI**, and token
   persistence is **not implemented**. When it lands it will use the OS keychain (Windows Credential
   Manager / macOS Keychain / Secret Service), never a config file.
2. **No auto-update.** This version never downloads or executes update packages on its own.
3. **Installers are unsigned.** Windows and macOS artifacts carry no code signature, so the OS may
   warn about them.

### What this project will not do

- Provide any way to bypass authentication
- Collect telemetry, or send account-related data anywhere other than official endpoints
- Commit private keys, certificates or tokens to the repository
