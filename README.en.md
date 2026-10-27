<p align="center">
  <img src="assets/branding/app-icon.png" width="120" alt="Qingci Craft Launcher">
</p>

# Qingci Craft Launcher

[中文](README.md) | **English**

[![CI](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci-main.yml/badge.svg)](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci-main.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

A cross-platform, lightweight Minecraft: Java Edition launcher built with **Tauri 2 + Rust**.

> Status: **it installs, launches, manages mods and updates itself.** The full path for vanilla
> versions (26.3, plus older releases such as 1.7.10 with the legacy asset layout) works end to end:
> fetch manifest → download ~598 MB → pick Java → assemble command → launch → stream logs → stop.
> Fabric is selectable per version; the mods tab supports search, install (required dependencies
> pulled in automatically), removal and update checks. Abnormal exits produce a crash summary that
> also reads the game's own `crash-reports/`. Installed builds update in place: check, download,
> verify the signature, install silently, relaunch.
> Roadmap: [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md) · Changes: [CHANGELOG.md](CHANGELOG.md).

## Feature status

| Capability | Status | Notes |
|---|---|---|
| Version manifest | ✅ | Derived from Mojang's official `version.json`; no hardcoded version list |
| Install & verify | ✅ | Version files, libraries, assets, natives; SHA-1 verified per file |
| Java selection | ✅ | Picks by version requirement; manual override is validated, not trusted blindly |
| Launch & logs | ✅ | Streams stdout/stderr live; "Stop" really terminates the process |
| Settings | ✅ | Game directory, offline player name, max heap, Java runtime, download source |
| Fabric loader | ✅ | Version → loader → install → launch; the merged profile is persisted |
| Mod management | ✅ | Modrinth search / install / remove / update check; required dependencies installed automatically |
| Chinese names & filters | ✅ | Local name database, Chinese keyword aliases, 19 localised categories |
| Crash summary | ✅ | Non-zero exit produces a verdict; reads `crash-reports/` when present |
| Auto-update | ✅ | Installed builds check automatically; nothing installs without a valid signature |
| Microsoft login | ✅ | Device code flow (the launcher never sees the password); tokens live in the OS keychain and refresh silently at startup |
| Instances | ✅ | Multiple independent game directories, each with its own version selection |
| Forge / NeoForge | ⬜ | See roadmap |

## Design principles

- **Lightweight**: uses the system WebView. Portable executable is 12.7 MB, NSIS installer 3.8 MB.
- **Cross-platform**: Windows 10+ / macOS 12+ / Linux (glibc 2.31+); CI builds and tests on all three.
  Published artifacts cover Windows and Linux.
- **Testable**: core logic (version parsing, rule evaluation, argument assembly, auth protocols, mod
  resolution, crash report parsing) lives in GUI-free Rust crates — 312 Rust tests + 132 frontend
  tests in CI.
- **Manifest-driven**: versions, libraries and JVM arguments are all derived from official manifests; no version list is hardcoded.

## Tech stack

| Layer | Choice |
|---|---|
| App framework | Tauri 2.12 (Rust 1.99.1 / edition 2024, MSRV 1.90) |
| Frontend | React 19 + TypeScript 7 + Vite 8 + Tailwind CSS 4 |
| Async & network | tokio + reqwest (rustls) |
| Frontend quality | vitest + Testing Library; oxlint (not eslint — see below) |
| Update delivery | `tauri-plugin-updater` + Tencent Cloud COS (signature-verified, see [docs/RELEASE.en.md](docs/RELEASE.en.md)) |
| Credential storage | OS keychain: Windows Credential Manager / macOS Keychain / Secret Service (see [SECURITY.md](SECURITY.md)) |

> **Why oxlint instead of eslint**: `typescript-eslint` declares a peer dependency of
> `typescript <6.1.0`, while this project uses TS 7 (the native Go compiler).
> oxlint parses TypeScript itself and does not depend on the `typescript` package.

## Repository layout

```
crates/          Rust domain layer (all testable without a GUI)
  qcl-core       Version parsing, rule evaluation, launch command assembly (pure logic)
  qcl-net        Download planning, executor, HTTP download with SHA-1 verification
  qcl-install    Install orchestration: version files, assets, natives, Fabric merging
  qcl-java       Java detection and on-demand installation
  qcl-runtime    Game process launch, log forwarding, termination, crash-report parsing
  qcl-platform   Host platform probing (OS version determines GC flags)
  qcl-auth       Microsoft device-code login and Xbox auth chain (no token persistence)
  qcl-mods       Modrinth search/install/remove/update, dependency resolution, Chinese name data
  qcl-cli        Headless entry point (smoke tests and debugging)
src-tauri/       Tauri app layer (thin adapter: IPC and state only)
src/             React 19 frontend
docs/            Development plan, release guide and architecture decision records
assets/          Brand assets (icon sources, mod name database)
```

## Installation

Download from [Releases](https://github.com/luoqingciya/Qingci-Craft-Launcher/releases):

| Platform | File | Size | Notes |
|---|---|---|---|
| Windows | `QingciCraftLauncher_*_x64-setup.exe` | 3.8 MB | NSIS installer; installs per-user, no admin rights; supports auto-update |
| Windows | `QingciCraftLauncher_*_x64-portable.exe` | 12.7 MB | No install, double-click to run; game directory sits next to the exe; **no auto-update** |
| Linux | `QingciCraftLauncher_*_amd64.deb` | 5.2 MB | Debian / Ubuntu package; depends on the system webkit2gtk |
| Linux | `QingciCraftLauncher_*_x86_64.AppImage` | 83.5 MB | No install; self-contained runtime |

The Linux artifacts and the Windows portable build are **GitHub Releases only** and take no part in
auto-update — the update server carries just the Windows installer.

**macOS is not published yet**: an unsigned dmg is blocked by Gatekeeper, so proper distribution
needs a paid developer account and notarization.

The size gap comes from **how much of the WebView is reused**: Windows uses the bundled WebView2
runtime, the deb relies on `libwebkit2gtk-4.1` from the distribution, while an AppImage has to carry
the entire WebKitGTK + JavaScriptCore + GTK3 stack to run on any distribution — that stack alone is
over 70 MB. Pick the deb if size matters, the AppImage if you want "download and double-click".

**The download source is switchable**: Settings offers Mojang's official endpoints (default) or the
[BMCLAPI](https://bmclapidoc.bangbang93.com/) mirror. A failing mirror **falls back to official
automatically** — it is a faster path, not a single point of failure. When the mirror is in use the UI
credits it, which BMCLAPI's terms also require.

The default game directory is **`game/` inside the install directory** (portable layout:
copy the whole folder and your worlds move with it). If that location is not writable,
the launcher falls back to the app data directory and explains why in the UI.

**The installed build and the portable build keep separate settings.** The installed one uses
`%APPDATA%\cn.luoqingciya.qcl\`; the portable one writes `settings.json` next to the exe. They share
the same application identifier, and used to share a single config file — which meant a version
picked in one showed up in the other. That is no longer the case.

**Only one instance may run at a time**: double-clicking again focuses the existing window instead of
opening a second one. Note that the installed and portable builds share the application identifier, so
they exclude each other too — double-clicking the portable build while the installed one is running
just brings the installed window forward.

### Auto-update

An installed build checks for updates at startup and stays silent when there is none. When a newer
version exists, the About page shows it with its release notes; "Download and install" then:

1. downloads the package from COS,
2. **verifies a minisign signature against a public key compiled into the binary** — a mismatch
   aborts the install,
3. installs silently and relaunches.

Portable builds have no uninstaller, and Windows will not let a running exe replace itself, so they
are pointed at the download page instead.

Setup and key management: [docs/RELEASE.en.md](docs/RELEASE.en.md).

## Development

Requirements:

- Rust **1.99.1** (pinned by `rust-toolchain.toml`; `rustup` installs it automatically)
- Node.js ≥ 22 and pnpm ≥ 10
- Windows: MSVC Build Tools. Linux: `libwebkit2gtk-4.1-dev` and friends (see CI config)

Common commands:

```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features

# Frontend (lint + unit tests + type check + production build)
pnpm install
pnpm lint
pnpm test
pnpm build

# Desktop app (dev mode with hot reload)
pnpm tauri dev

# Package installers
pnpm tauri build              # produces both NSIS and MSI
pnpm tauri build --bundles nsis

# Headless entry point: install and print the launch command without starting the game
cargo run -p qcl-cli -- 26.3 --dry-run
```

## Contributing

This project uses a strict PR workflow: every change (including docs) goes through
branch → PR → green CI → review → squash merge.
See [CONTRIBUTING.en.md](CONTRIBUTING.en.md) ([中文](CONTRIBUTING.md)).

Release and auto-update setup, key management and troubleshooting:
[docs/RELEASE.en.md](docs/RELEASE.en.md) ([中文](docs/RELEASE.md)).

## Disclaimer

This is an unofficial launcher, not affiliated with Mojang Studios or Microsoft.
"Minecraft" is a trademark of Mojang Studios. This project does not distribute any game
files, libraries or assets — everything is fetched at runtime from official public endpoints.
It does not and will not provide any way to bypass authentication.

## License

[GPL-3.0](LICENSE) with **additional terms under GPLv3 Section 7**
([English](ADDITIONAL-TERMS.en.md) · [中文](ADDITIONAL-TERMS.md)):

- Copyright and license notices must be preserved when distributing
- **Modified versions must be renamed**; they must not imply they are the original or endorsed by the author

Verbatim copies of the unmodified work may keep the original name — the additional terms
do not restrict verbatim redistribution, which GPLv3 permits.
