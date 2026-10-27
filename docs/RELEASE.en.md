# Release and Auto-Update

English | [中文](RELEASE.md)

Qingci updates are delivered as "GitHub Actions builds → Tencent Cloud COS serves". CI
builds the NSIS installer, signs it with a private key, and uploads it to COS together
with the update manifest `latest.json`. On startup the client reads the manifest, and if
a newer version exists it downloads the installer and **verifies the signature against
the bundled public key before installing**.

```
tag v0.1.2
   │
   ▼
GitHub Actions
   ├─ prepare   verify the three version strings + the signing key
   ├─ build     matrix: Windows → NSIS / Linux → AppImage + deb (both signed)
   └─ publish   merge artifacts → write latest.json → upload to COS → create the Release
                   │
                   ├─ COS (only what auto-update needs)
                   │    ├─ QingciCraftLauncher_0.1.2_x64-setup.exe
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

The manifest only carries `windows-x86_64`: the Linux AppImage is 83 MB and does not participate in
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

The key file is a base64-encoded minisign key with **exactly one line** (the
`untrusted comment:` markers live inside the base64 payload, so you will not see them).
Copy the whole thing; do not add a trailing newline.

The key was generated without a password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is not
needed. Add it if you ever regenerate with a password.

> Why the password is optional: key and password would live in the same secret store, so
> the password adds no real protection there. It only matters if you keep the key file
> somewhere else, e.g. offline.

### 3. COS

- Bucket `qcl-1331847184`, region `ap-guangzhou`, prefix `releases/`
- Custom domain `qcl.luoqingciya.cn`, **HTTPS required** — the client rejects http endpoints
- The bucket must not enable a referer allowlist, or the client cannot fetch the manifest

**How read access is granted.** Bucket ACL and object ACL are two separate things:

| Option | Bucket ACL | What makes `releases/` anonymously readable |
| --- | --- | --- |
| A (current) | public-read / private-write | the bucket ACL itself |
| B (tighter, recommended) | **private** | the upload script sets `public-read` per object |

Switching from A to B is safe because `scripts/publish-cos.mjs` already sends
`ACL: public-read` on every upload. Two things to watch:

1. **Objects uploaded before the switch may become private with the bucket.** Verify immediately
   afterwards: `curl -I https://qcl.luoqingciya.cn/releases/latest.json` must return 200. If it
   returns 403, set those two objects to public-read in the console.
2. If the bucket disables object-level ACLs, option B is not available — `putObject` will fail. Use a
   bucket policy instead: resource `qcl-1331847184/releases/*` (**plural `releases`**), action "read
   operations", and do not enable "list objects". A one-letter typo in the prefix matches nothing,
   and while the bucket is public-read that mistake is completely invisible — it only shows up as a
   403 the day you switch to a private bucket.

Verify the permissions with these three probes; together they tell "is the bucket really private"
apart from "is the prefix really effective":

```bash
# 1. the manifest itself: 200 means it works
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/latest.json
# 2. a missing key inside the prefix: 404 = anonymous read allowed here
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/nope.txt
# 3. a missing key outside the prefix: 403 = the bucket really is private
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/nope.txt
```

The key insight: **a private bucket answers 403 for a missing object, while a readable prefix answers
404**. Testing only probe 1 cannot distinguish "permissions are wrong" from "the file is not uploaded
yet".

## Releasing

```bash
# 1. bump the version in three places (CI fails if they disagree)
#    package.json               "version"
#    src-tauri/tauri.conf.json  "version"      ← this is what the client compares
#    Cargo.toml                 [workspace.package] version
#      └─ reaches the game's --version and the HTTP User-Agent via qcl_core::LAUNCHER_VERSION

# 2. regenerate the changelog (include it in the same PR, do not fix it afterwards)
git cliff --config cliff.toml -o CHANGELOG.md

# 3. commit and tag (replace the version with the one you are shipping)
git commit -am "chore(release): 0.1.3"
git tag v0.1.3
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
| GitHub Releases | all of the above, plus Linux AppImage and deb | manual downloads |

`latest.json` therefore carries **only** a `windows-x86_64` entry. The Linux artifacts stay on GitHub
Releases: the AppImage is 83 MB and does not participate in auto-update, so putting it on COS would
cost bandwidth for nothing.

- **Windows**: NSIS, 3.5 MB, reuses the system WebView2; auto-update installs silently and relaunches.
- **Linux AppImage**: 83 MB. Self-contained — it carries the whole WebKitGTK + JavaScriptCore + GTK3
  stack (530 files once extracted), which is why it is over ten times the size of the deb.
- **Linux deb**: 4.9 MB, depends on the distribution's `libwebkit2gtk-4.1`.
- **Linux in general**: no auto-update. The About page detects Linux and points at the download page
  instead of showing a button that would only fail.
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
