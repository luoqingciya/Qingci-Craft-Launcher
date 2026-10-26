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
4. **Mod download verification**: mods from Modrinth must carry a valid SHA-1 before anything is
   downloaded; remote file names are rejected if they contain path separators or `..`, so nothing is
   written outside `mods/`.
5. **Signed update packages**: the updater only reads
   `https://qcl.luoqingciya.cn/releases/latest.json`, and verifies the downloaded package against a
   **public key compiled into the binary** (minisign). A signature mismatch aborts the install. The
   version announced in the manifest must also match the version recorded inside the signature —
   otherwise a validly signed but older build could be paired with a forged version number to force a
   downgrade.
6. **No tokens in logs**: the auth layer (`qcl-auth`) never prints tokens and never persists them.

### Not yet implemented (current risk)

1. **No credential storage.** Only offline accounts are supported right now, so there are no tokens
   to store. The Microsoft login protocol layer is complete but **not wired into the UI**, and token
   persistence is **not implemented**. When it lands it will use the OS keychain (Windows Credential
   Manager / macOS Keychain / Secret Service), never a config file.
2. **No code signing for executables.** Windows artifacts carry no Authenticode signature; macOS
   artifacts are not codesigned or notarized. The OS may warn about them.

> Keep the two kinds of signing apart: **minisign signatures on update packages** exist today and
> guarantee that an update really came from this project. **Code signing of the executables**
> (Authenticode / codesign) does not exist yet; it mainly affects OS trust prompts, not the integrity
> of the update channel.

### Trust boundary of the update mechanism

Updating is, in essence, "fetch an executable from the network and run it", so it has exactly one
root of trust: **the public key compiled into the program**. Two consequences worth knowing:

- If the private key leaks, its holder can sign arbitrary updates for every released client. The key
  lives only on the maintainer's machine and in an encrypted GitHub Actions secret; it is never
  committed.
- The manifest (`latest.json`) is not itself signed and travels over HTTPS. The only parts an attacker
  could tamper with are the version number and the release notes, because the package must still pass
  signature verification. A forged version number is stopped by the consistency check described
  above.

### What this project will not do

- Provide any way to bypass authentication
- Collect telemetry, or send account-related data anywhere other than official endpoints
- Commit private keys, certificates or tokens to the repository
