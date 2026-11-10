# Release and Auto-Update

[中文](RELEASE.md) | **English**

Qingci updates are delivered as "GitHub Actions builds → GitHub Release serves". CI builds the
installers for all three platforms, signs them with a private key, and publishes them together with
the update manifest `latest.json` on GitHub Releases. On startup the client reads the manifest, and if
a newer version exists it downloads the installer and **verifies the signature against
the bundled public key before installing**.

```
tag v0.1.14           ← 0.1.14 as the worked example
   │
   ▼
GitHub Actions
   ├─ prepare   verify the three version strings + the signing key
   ├─ build     matrix: Windows → NSIS / Linux → AppImage + deb / macOS → dmg + .app.tar.gz (all signed)
   └─ publish   merge artifacts → write latest.json → create the Release
                   │
                   └─ GitHub Release (everything)
                        ├─ QingciCraftLauncher_0.1.14_x64-setup.exe + .sig
                        ├─ ..._x86_64.AppImage + .sig
                        ├─ ..._universal.app.tar.gz + .sig
                        ├─ ..._universal.dmg / ..._amd64.deb / portable build
                        └─ latest.json
   │
   ▼
client startup → GET github.com/.../releases/latest/download/latest.json
   ├─ not newer → do nothing (no interruption)
   └─ newer → badge on the About tab → "Download and install"
              → signature check → silent install → relaunch
```

The manifest covers **three platforms, four targets**: `windows-x86_64` (NSIS), `linux-x86_64`
(AppImage), `darwin-aarch64` and `darwin-x86_64` (the same universal `.app.tar.gz`). deb packages are
managed by the system package manager and the portable build does not self-update, so neither has an
entry.

## Update channel

GitHub Release is the only channel. COS (`qcl.luoqingciya.cn`) was shut down on 2026-11-10 and has
been removed from the workflow and the endpoint list — `scripts/publish-cos.mjs` was deleted with it;
the repository carries no COS configuration any more.

**The cost, stated plainly**: clients ≤ 0.1.14 have only COS in their endpoint list and will
**never see** a newer release. That is the accepted price of shutting COS down, not a defect;
affected users need one manual download from GitHub Releases.

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

The COS configuration (bucket, custom domain, anonymous-read policy) was retired together with
COS on 2026-11-10 and is no longer needed — see "Platform status" below.

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

The `Release` workflow then builds, signs, generates the manifest and
creates a GitHub Release with the installers and `.sig` files attached.

**Never reuse a tag.** Clients compare against the version in `latest.json`; republishing
the same version means users who already upgraded will not update again. Bump the version.

### Verify before publishing

A manual `workflow_dispatch` run only builds and prints `latest.json` — it does **not** create a
Release, which makes it suitable for validating a build before tagging.

### Pre-release smoke (install pipeline)

The UI cannot reproduce an "empty instance", yet every blocking bug so far only showed up
in an empty directory (missing vanilla jar, loader kind out of sync, clicking a stale
loader list). Run the CLI instead:

```bash
# Empty instance + Forge: covers the "install vanilla first, then the loader" ordering
cargo run -p qcl-cli -- 1.20.1 --dir /tmp/qcl-smoke-forge --fresh \
  --loader forge:47.2.0 --expect "Forge mod loading"

# Empty instance + Fabric
cargo run -p qcl-cli -- 1.20.1 --dir /tmp/qcl-smoke-fabric --fresh \
  --loader fabric:0.15.0 --expect "Setting user"
```

- `--fresh` requires an empty directory (missing or empty) and **never clears it for you**.
- `--expect <text>` waits for that text in the log; a hit stops the game and exits 0,
  a timeout exits non-zero.
- `--dry-run` installs and prints the launch command without starting the game.
- The order matches the desktop app (vanilla → loader → top up the merged version), so it
  catches the same class of mistake.

## Platform status

GitHub Release is the only distribution and update channel:

| Artifact | Purpose |
| --- | --- |
| Windows NSIS installer + `.sig` | auto-update (`windows-x86_64`) |
| Linux AppImage + `.sig` | auto-update (`linux-x86_64`); deb does not participate |
| macOS universal `.app.tar.gz` + `.sig` | auto-update (one entry each for `darwin-aarch64` and `darwin-x86_64`) |
| macOS dmg, Windows portable, Linux deb | manual download only |

`latest.json` itself is published as a Release asset and read by clients through
`releases/latest/download/latest.json`.

Sizes below are measured from the 0.1.5 artifacts and drift a little between versions (more so when
the frontend changes) — trust the actual artifacts, not these numbers.

- **Windows installer**: NSIS, 3.8 MB, reuses the system WebView2; auto-update installs silently and relaunches.
- **Windows portable**: 12.7 MB, the very exe that lives inside the installer — double-click to run, game
  directory next to the exe. It does receive update prompts; installing means upgrading to the NSIS
  build (the portable directory itself is not replaced).
- **macOS universal dmg + `.app.tar.gz`**: 12.1 MB, built with `--target universal-apple-darwin`, so one
  file carries both arm64 and x86_64 — no architecture picking. **Unsigned and not notarized**: without
  a paid developer account, the first launch needs right-click → Open, or
  `xattr -dr com.apple.quarantine`; the README lists three ways. The dmg is for first installs;
  auto-update ships as `.app.tar.gz` — it is unpacked with tar+gz and **does not depend** on the
  `zip` feature disabled by RUSTSEC-2026-0367/0368 (long assumed otherwise; corrected after reading
  the tauri-plugin-updater 2.14.0 sources, see `src-tauri/Cargo.toml`).
- **Linux AppImage**: 83.5 MB. Self-contained — it carries the whole WebKitGTK + JavaScriptCore + GTK3
  stack (530 files once extracted), which is why it is over ten times the size of the deb.
  Auto-update replaces the AppImage file in place.
- **Linux deb**: 5.3 MB, depends on the distribution's `libwebkit2gtk-4.1`. **No update entry**: deb
  files belong to the package manager — having the updater replace files under `/usr/bin` would
  require root and fight dpkg, so deb installs keep updating manually.
- macOS artifacts are still not Apple-signed or notarized (no paid developer account); the minisign
  signatures are generated as usual, and updates are unaffected by Gatekeeper (the quarantine
  attribute only appears on files downloaded through a browser).

## Client behaviour

- One silent check at startup; a red dot appears on the About tab only when an update
  exists. Network failures stay silent — a "check failed" banner on a freshly opened
  launcher just makes people think the app is broken
- The About page also has a manual "Check for updates" that surfaces the error
- There is a single endpoint (GitHub Release); its `releases/latest/download/` is a 302
  redirect that finally lands on the assets of that Release
- A signature mismatch **blocks installation** (`requireSignedVersion` additionally
  rejects a downgrade attack where the manifest version disagrees with the signed version)

## Troubleshooting

| Symptom | Check |
| --- | --- |
| About page always says "up to date" | `curl -L https://github.com/luoqingciya/Qingci-Craft-Launcher/releases/latest/download/latest.json` — 200? version higher than local? |
| Check fails | is `latest.json` actually an asset of that Release; is GitHub reachable (use a proxy if direct
  connection fails) |
| ≤ 0.1.14 no longer prompts for updates | expected — they only read the shut-down COS; download once manually |
| Reaches 100% then fails | signature mismatch: private key and `pubkey` are not a pair, or `signature` is not the raw `.sig` content |
| Linux update fails | only the AppImage flavour self-updates; deb installs update through the package manager |
| Version unchanged after install | installed into a different directory (portable build treated as installed), or an older MSI install is also present |
| Switching to BMCLAPI did not help, or still fails | a failing mirror **falls back to official automatically**. BMCLAPI's metadata endpoints 302 to its own CDN (`bmclapi*.sakura.sld.tw`), which some networks cannot reach; the Forge maven path goes through CERNET/USTC and usually works |
