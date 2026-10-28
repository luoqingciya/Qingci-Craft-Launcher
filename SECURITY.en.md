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
   letters and backslashes, so nothing is written outside the game directory. Directory names derived
   from an instance name keep only letters, digits and `-` `_`, so a `../` in the name cannot escape
   `instances/` either.
3. **No unverified execution**: the game is launched only from official artifacts; loader (Fabric)
   libraries are verified the same way.
4. **Mod download verification**: mods from Modrinth must carry a valid SHA-1 before anything is
   downloaded; remote file names are rejected if they contain path separators or `..`, so nothing is
   written outside `mods/`. **Dependencies installed automatically go through the same path** — being
   a convenience is not a reason to skip verification.
5. **Signed update packages**: the updater only reads
   `https://qcl.luoqingciya.cn/releases/latest.json`, and verifies the downloaded package against a
   **public key compiled into the binary** (minisign). A signature mismatch aborts the install. The
   version announced in the manifest must also match the version recorded inside the signature —
   otherwise a validly signed but older build could be paired with a forged version number to force a
   downgrade.
6. **Tokens only go to the OS keychain**: the auth layer (`qcl-auth`) never prints tokens. The only
   secret persisted is the **Microsoft refresh token** (a Minecraft access token lives under an hour,
   so storing it is pointless); it goes into the OS keychain (Windows Credential Manager / macOS
   Keychain / Linux Secret Service) and **never into a config file**, which is plaintext. When no
   keychain is available the launcher degrades to no persistence — login still works, it just has to
   be repeated next launch.

### Trust boundary of the download source

Downloads come from Mojang's official endpoints by default. Settings can switch to the
[BMCLAPI](https://bmclapidoc.bangbang93.com/) mirror — a third-party service, so **switching means
handing that trust to it**:

- The mirror only rewrites addresses; verification is unchanged: every file is still checked against
  the SHA-1 in the manifest.
- But **the manifest itself may also come from the mirror**. An attacker in control of the mirror
  could change a file and its digest together, in which case the SHA-1 check proves the download was
  not corrupted, not that it came from Mojang.
- So this trades **authenticity for availability**: the mirror only makes sense when the official
  endpoints are unreachable, and official stays the default.
- **A failing mirror falls back to official automatically**: on connection failure, an explicit failure
  status, **and on a SHA-1 mismatch after download**. A redirect is not a failure — that is normal
  mirror behaviour. The last case matters most: if the mirror answers 200 with the wrong bytes,
  retrying the same mirror can only ever yield the same wrong bytes, which would turn checksum
  verification into "it reports the problem but never recovers". This is not just about UX either:
  BMCLAPI's metadata endpoints 302 to `bmclapi*.sakura.sld.tw`, and those nodes reset outright on some
  networks. Without the fallback, switching to the mirror would leave the user unable to even list
  versions.

The UI credits the mirror in **two** places: in Settings when it is selected, and next to the download progress bar. The latter is not optional — BMCLAPI's terms require the source to be shown "in the download UI or another visible part", and a note buried in Settings is invisible to anyone who never opens it.

### Not yet implemented (current risk)

1. **No protection beyond the OS keychain.** The refresh token is protected by the operating system;
   any process in the same user session that could read it could do plenty of other things anyway.
   There is no extra application-level encryption, because the key would have to live on the same
   machine — that only moves the problem.
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

### Trust boundary of the login flow

Sign-in uses the OAuth 2.0 **device authorization grant** (RFC 8628), not the authorization code
flow: the user opens a Microsoft page in their own browser and types a short code, so the launcher
never sees the password.

**Why there is no PKCE and no `state`** — both belong to the authorization code flow and do not exist
in the device flow:

| Parameter | Problem it solves | Why the device flow does not need it |
| --- | --- | --- |
| PKCE (`code_challenge` / `code_verifier`) | authorization code intercepted during the redirect | there is no redirect and no authorization code |
| `state` | CSRF: proving the request came from us by echoing the value back on redirect | there is no redirect to echo it on |

So "no PKCE/state" is not an omission — neither is defined by RFC 8628.

**The client ID is not a secret.** Public clients have no client secret, and the ID appears in
plaintext in every request. Injecting it at compile time (`QCL_MS_CLIENT_ID`) only keeps the
maintainer's app registration out of the repository; **it is always extractable from the binary**
(`grep -a` finds it in a second). Actually preventing others from reusing it has to happen on the
Azure side, not in code.

**The risk this flow does have is device code phishing**: an attacker starts a device flow with their
own app and tricks a victim into entering the code, which authorises the victim's account to the
attacker's app. That happens outside the launcher and no client can prevent it; the only thing a user
can check is the app name shown on Microsoft's page. The UI therefore must:

- show the URL the user should visit and **never fold the code into a link**
  (`verification_uri_complete` would destroy the user's ability to tell who they are talking to)
- validate the URL with `qcl_auth::is_trusted_verification_uri` before opening it (https only, host
  must be a Microsoft domain) rather than trusting the address in the response
- respect the code's lifetime and stop polling when it expires

**No tokens are persisted today** (a deliberate constraint of `qcl-auth`). Persistence will use the OS
keychain (Windows Credential Manager / macOS Keychain / Secret Service), never a config file.

### What this project will not do

- Provide any way to bypass authentication
- Collect telemetry, or send account-related data anywhere other than official endpoints
- Commit private keys, certificates or tokens to the repository
