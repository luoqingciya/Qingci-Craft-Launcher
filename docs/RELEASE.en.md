# Release and Auto-Update

English | [中文](RELEASE.md)

Qingci updates are delivered as "GitHub Actions builds → Tencent Cloud COS serves". CI
builds the NSIS installer, signs it with a private key, and uploads it to COS together
with the update manifest `latest.json`. On startup the client reads the manifest, and if
a newer version exists it downloads the installer and **verifies the signature against
the bundled public key before installing**.

```
tag v0.1.1
   │
   ▼
GitHub Actions (windows-latest)
   ├─ pnpm tauri build --bundles nsis
   ├─ artifacts: QingciCraftLauncher_0.1.1_x64-setup.exe + .sig
   ├─ scripts/make-latest-json.mjs → latest.json
   └─ scripts/publish-cos.mjs → cos://qcl-1331847184/releases/
                                   ├─ QingciCraftLauncher_0.1.1_x64-setup.exe
                                   ├─ ...exe.sig
                                   └─ latest.json
   │
   ▼
client startup → GET https://qcl.luoqingciya.cn/releases/latest.json
   ├─ not newer → do nothing (no interruption)
   └─ newer → badge on the About tab → "Download and install"
              → signature check → silent install → relaunch
```

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
| `TAURI_SIGNING_PRIVATE_KEY` | **the whole private key file**, both comment lines included |

The key was generated without a password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is not
needed. Add it if you ever regenerate with a password.

> Why the password is optional: key and password would live in the same secret store, so
> the password adds no real protection there. It only matters if you keep the key file
> somewhere else, e.g. offline.

### 3. COS

- Bucket `qcl-1331847184`, region `ap-guangzhou`, prefix `releases/`
- Custom domain `qcl.luoqingciya.cn`, **HTTPS required** — the client rejects http endpoints
- Object ACL: the upload script sets `public-read` per object. If the bucket disables
  object-level ACLs, grant anonymous read on the bucket or on this prefix instead
- The bucket must not enable a referer allowlist, or the client cannot fetch the manifest

## Releasing

```bash
# 1. bump the version in two places (CI fails if they disagree)
#    package.json               "version"
#    src-tauri/tauri.conf.json  "version"

# 2. commit and tag
git commit -am "chore: release 0.1.1"
git tag v0.1.1
git push origin main --tags
```

The `Release` workflow then builds, signs, generates the manifest, uploads to COS and
creates a GitHub Release with the installer and `.sig` attached.

**Never reuse a tag.** Clients compare against the version in `latest.json`; republishing
the same version means users who already upgraded will not update again. Bump the version.

### Verify before publishing

`workflow_dispatch` defaults to `dry_run = true`: it builds and prints `latest.json` plus
the objects it would upload, without touching COS or creating a Release. Run it once
after configuring secrets.

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
