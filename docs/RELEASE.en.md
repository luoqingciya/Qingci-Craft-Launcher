# Release and Auto-Update

[中文](RELEASE.md) | **English**

Qingci updates are delivered as "GitHub Actions builds → Tencent Cloud COS serves". CI
builds the NSIS installer, signs it with a private key, and uploads it to COS together
with the update manifest `latest.json`. On startup the client reads the manifest, and if
a newer version exists it downloads the installer and **verifies the signature against
the bundled public key before installing**.

```
tag v0.1.4            ← 0.1.4 as the worked example
   │
   ▼
GitHub Actions
   ├─ prepare   verify the three version strings + the signing key
   ├─ build     matrix: Windows → NSIS / Linux → AppImage + deb (both signed)
   └─ publish   merge artifacts → write latest.json → upload to COS → create the Release
                   │
                   ├─ COS (only what auto-update needs)
                   │    ├─ QingciCraftLauncher_0.1.4_x64-setup.exe
                   │    ├─ ...exe.sig
                   │    └─ latest.json            ← uploaded last
                   │
                   └─ GitHub Release (everything)
                        └─ the three above + AppImage + AppImage.sig + deb
   │
   ▼
client startup → GET https://qcl.luoqingciya.cn/releases/latest.json
   ├─ not newer → do nothing (no interruption)
   └─ newer → badge on the About tab → "Download and install"
              → signature check → silent install → relaunch
```

The manifest only carries `windows-x86_64`: the Linux AppImage is 83.5 MB and does not participate in
auto-update, so putting it on COS would cost bandwidth for nothing — it stays on GitHub Releases
together with the deb.

## One-time setup

### 1. Signing key

Every update must be signed; the public key is compiled into the client. **Lose the
private key and you can never ship an update that existing clients accept** — every user
would have to reinstall manually.

The key pair is already generated on this machine:

| File | Purpose |
| --- | --- |
| `%USERPROFILE%\.tauri\qcl-updater.key` | private key, paste into a GitHub secret, **never commit** |
| `%USERPROFILE%\.tauri\qcl-updater.key.pub` | public key, already in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey` |

Signing only happens at release time: `createUpdaterArtifacts` lives in
`src-tauri/tauri.release.json` and is merged in by the release workflow via `--config`.
A contributor running `pnpm tauri build` locally needs no key at all.

To regenerate (this invalidates every client already released):

```bash
pnpm tauri signer generate -w ~/.tauri/qcl-updater.key --ci
```

### 2. Repository configuration

Settings → Secrets and variables → Actions:

| Name | Value |
| --- | --- |
| `TENCENT_SECRET_ID` | Tencent Cloud API secret id |
| `TENCENT_SECRET_KEY` | Tencent Cloud API secret key |
| `TAURI_SIGNING_PRIVATE_KEY` | **the whole private key file**: a single base64 line, no line breaks |
| `QCL_MS_CLIENT_ID` | the "Application (client) ID" from the Azure app registration |

`QCL_MS_CLIENT_ID` is injected **at compile time** by `src-tauri/build.rs` (`cargo:rustc-env`), so
it never appears in the repository. Missing it does not break the build, but the resulting binary
silently ships without Microsoft login — which is why the release workflow treats it as a
precondition and fails early.

**It is not a secret, so do not treat it as one.** Public OAuth clients (device code flow) have no
client secret, and the ID appears in plaintext in every request. Compile-time injection only keeps
the maintainer's app registration out of the repository; **it is always present in the binary** —
`grep -a <id> <binary>` finds it in a second. Actually preventing others from reusing the ID has to
happen on the Azure side, not in code.

Contributors can build and run everything without it; only login is unavailable (the UI says so).
To test login locally, register your own Azure app and run
`QCL_MS_CLIENT_ID=xxx pnpm tauri dev`.

The key file is a base64-encoded minisign key with **exactly one line** (the
`untrusted comment:` markers live inside the base64 payload, so you will not see them).
Copy the whole thing; do not add a trailing newline.

The key was generated without a password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is not
needed. Add it if you ever regenerate with a password.

> Why the password is optional: key and password would live in the same secret store, so
> the password adds no real protection there. It only matters if you keep the key file
> somewhere else, e.g. offline.

### 3. COS

- Bucket `qcl-1331847184`, region `ap-guangzhou`, **private read/write**, prefix `releases/`
- Custom domain `qcl.luoqingciya.cn`, **HTTPS required** — the client rejects http endpoints
- The bucket must not enable a referer allowlist, or the client cannot fetch the manifest

**How read access is granted.** The bucket is **private read/write**; `releases/` is anonymously
readable because two layers both hold:

| Layer | Current state | Evidence |
| --- | --- | --- |
| Object ACL | set to `public-read` on upload | `ACL: "public-read"` in `scripts/publish-cos.mjs` |
| Bucket policy | anonymous read on the `releases/*` prefix, **"list objects" not granted** | `releases/nope.txt` answers 404, not 403 |

The redundancy is deliberate, but when debugging a 403 remember there are **two** places to look —
fixing only one may change nothing.

Do not set the bucket itself to "public-read": that would make **everything** in the bucket
anonymously readable, while only `releases/` needs to be public.

Two preconditions:

1. **The bucket must not disable object-level ACLs.** If it does, `putObject` with an ACL fails
   outright and every upload breaks; in that case the bucket policy is the only layer, and the object
   ACL must be dropped.
2. The policy resource is `qcl-1331847184/releases/*` (**plural `releases`**). A one-letter typo in
   the prefix matches nothing and **does not fail loudly** — the object ACL still serves the files,
   and the mistake only surfaces as a 403 once that ACL layer stops working.

These four probes tell whether the permissions are really what the table above claims (verified
2026-10-27):

```bash
# 1. the manifest itself: 200 means it works
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/latest.json
# 2. a missing key inside the prefix: 404 = anonymous read allowed here
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/nope.txt
# 3. a missing key outside the prefix: 403 = the bucket really is private
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/nope.txt
# 4. anonymous listing: 403 means listing is not open (otherwise anyone can enumerate releases)
curl -s -o /dev/null -w "%{http_code}\n" "https://qcl.luoqingciya.cn/?prefix=releases/"
```

The key insight: **a private bucket answers 403 for a missing object, while a readable prefix answers
404**. Testing only probe 1 cannot distinguish "permissions are wrong" from "the file is not uploaded
yet".

Probe 4 is easy to miss: `/releases/` (with the trailing slash) returning 200 is COS's directory
placeholder and does **not** mean the bucket can be listed; a real listing carries a `?prefix=` query
string, and only a 403 there proves listing is closed.

## Releasing

```bash
# 1. bump the version in three places (CI fails if they disagree)
#    package.json               "version"
#    src-tauri/tauri.conf.json  "version"      ← this is what the client compares
#    Cargo.toml                 [workspace.package] version
#      └─ reaches the game's --version and the HTTP User-Agent via qcl_core::LAUNCHER_VERSION

# 2. regenerate the changelog (include it in the same PR, do not fix it afterwards)
#    --tag labels the unreleased section with the version being shipped. Without it
#    the tag does not exist yet, so git-cliff can only write "unreleased", and fixing
#    that afterwards costs another PR.
git cliff --config cliff.toml --tag v0.1.4 -o CHANGELOG.md

# 3. commit and tag (0.1.4 is the worked example; use the version you are shipping)
git commit -am "chore(release): 0.1.4"
git tag v0.1.4
git push origin main --tags
```

Step 2 is easy to forget — both v0.1.1 and v0.1.2 skipped it, leaving the changelog stuck on the
project's first five PRs until it was repaired afterwards. The reason is mundane: the rule lived in
CONTRIBUTING as "generate before a release" without being tied to any concrete step.

The `Release` workflow then builds, signs, generates the manifest, uploads to COS and
creates a GitHub Release with the installer and `.sig` attached.

**Never reuse a tag.** Clients compare against the version in `latest.json`; republishing
the same version means users who already upgraded will not update again. Bump the version.

### Verify before publishing

`workflow_dispatch` defaults to `dry_run = true`: it builds and prints `latest.json` plus
the objects it would upload, without touching COS or creating a Release. Run it once
after configuring secrets.

## Platform status

Two distribution channels, deliberately kept apart:

| Channel | Contents | Purpose |
| --- | --- | --- |
| COS (`qcl.luoqingciya.cn/releases/`) | Windows NSIS installer + `.sig` + `latest.json` | the auto-update channel |
| GitHub Releases | all of the above, plus the Windows portable build, the macOS dmg and Linux AppImage/deb | manual downloads |

`latest.json` therefore carries **only** a `windows-x86_64` entry. The macOS and Linux artifacts stay
on GitHub Releases: the AppImage is 83.5 MB, the dmg is 12 MB, and neither participates in
auto-update, so putting them on COS would cost bandwidth for nothing.

Sizes below are measured from the 0.1.5 artifacts and drift a little between versions (more so when
the frontend changes) — trust the actual artifacts, not these numbers.

- **Windows installer**: NSIS, 3.8 MB, reuses the system WebView2; auto-update installs silently and relaunches.
- **Windows portable**: 12.7 MB, the very exe that lives inside the installer — double-click to run, game
  directory next to the exe. No auto-update: replacing a running exe is not allowed on Windows.
- **macOS universal dmg**: 12.1 MB, built with `--target universal-apple-darwin`, so one file carries both arm64
  and x86_64 — no architecture picking. **Unsigned and not notarized**: without a paid developer
  account, the first launch needs right-click → Open, or `xattr -dr com.apple.quarantine`; the README
  lists three ways.
- **macOS uses its own release config** (`tauri.release-macos.json`) with `createUpdaterArtifacts`
  turned off. Two reasons: enabling it demands a signing key at build time, and unpacking
  `.app.tar.gz` needs the updater's `zip` feature, which is disabled because of two extraction
  advisories (RUSTSEC-2026-0367 / 0368) — see `src-tauri/Cargo.toml`.
- **Linux AppImage**: 83.5 MB. Self-contained — it carries the whole WebKitGTK + JavaScriptCore + GTK3
  stack (530 files once extracted), which is why it is over ten times the size of the deb.
- **Linux deb**: 5.3 MB, depends on the distribution's `libwebkit2gtk-4.1`.
- **macOS and Linux**: no auto-update. The About page detects them from the UA (Linux / Macintosh) and
  points at the download page instead of showing a button that would only fail.
- **macOS**: not published. Auto-update needs the updater's `zip` feature to unpack `.app.tar.gz`,
  and that feature is currently disabled because of two extraction vulnerabilities
  (RUSTSEC-2026-0367 / 0368) — see `src-tauri/Cargo.toml`. An unsigned dmg is also blocked outright
  by Gatekeeper, so proper distribution needs a paid developer account and notarization.

## Client behaviour

- One silent check at startup; a red dot appears on the About tab only when an update
  exists. Network failures stay silent — a "check failed" banner on a freshly opened
  launcher just makes people think the app is broken
- The About page also has a manual "Check for updates" that surfaces the error
- **Only installed builds auto-update**: a portable build has no `uninstall.exe`, and
  Windows will not let a running exe be replaced, so the UI points at the download page
- A signature mismatch **blocks installation** (`requireSignedVersion` additionally
  rejects a downgrade attack where the manifest version disagrees with the signed version)

## Troubleshooting

| Symptom | Check |
| --- | --- |
| About page always says "up to date" | `curl https://qcl.luoqingciya.cn/releases/latest.json` — 200? version higher than local? |
| Check fails | domain certificate, bucket permissions, manifest actually under `releases/` |
| Reaches 100% then fails | signature mismatch: private key and `pubkey` are not a pair, or `signature` is not the raw `.sig` content |
| Version unchanged after install | installed into a different directory (portable build treated as installed), or an older MSI install is also present |
| Switching to BMCLAPI did not help, or still fails | a failing mirror **falls back to official automatically**. BMCLAPI's metadata endpoints 302 to its own CDN (`bmclapi*.sakura.sld.tw`), which some networks cannot reach; the Forge maven path goes through CERNET/USTC and usually works |
