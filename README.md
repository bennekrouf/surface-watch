# SurfaceWatch

A local console that watches the **client-side attack surface** of the web apps
in a pentest engagement. It captures each target with
[REA](https://github.com/morluto/rea), normalises the traffic into a surface
(endpoints, parameters, statuses, security headers, third-party origins), and
tells you what changed between two builds, without the noise of hashed chunk
names, timing and images. Findings link to the REA evidence they rest on, and a
retest tells you whether each one is fixed.

Runs fully local. No LLM is involved.

| Edition | Best for | Built with |
|---|---|---|
| **SurfaceWatch** (Desktop) | Reviewing engagements, diffs and findings | [Dioxus](https://dioxuslabs.com/) (Rust) |
| **`surfacewatch-cli`** (CLI) | Scheduled watches from cron or CI, quick checks over SSH | Rust, same core |

> Status: early. The core, the CLI, all five desktop tabs (Surface, Watch,
> Findings, Captures, Readiness) and the engagement and target forms work.
> Not yet: editing an engagement or target after creation, release packaging.

## Install

| Platform | Download |
|---|---|
| **macOS** (Apple Silicon) | [`surfacewatch-macos-arm64.dmg`](https://mayorana.ch/downloads/surfacewatch/latest/surfacewatch-macos-arm64.dmg): open it, drag **SurfaceWatch** to Applications. Signed and notarized. |
| **Windows** | [`surfacewatch-setup.exe`](https://mayorana.ch/downloads/surfacewatch/latest/surfacewatch-setup.exe): run the installer (no admin rights needed). |
| **Linux** (x86_64) | `curl -L https://mayorana.ch/downloads/surfacewatch/latest/surfacewatch-linux-x86_64.tar.gz \| tar xz && cd surfacewatch-linux-x86_64 && sudo ./setup-linux.sh && ./surfacewatch` |

The `surfacewatch-cli` command line for each platform is attached to every
[release](https://mayorana.ch/en/apps/surfacewatch/releases). The app tells
you when a newer version is out.

## Releasing

```bash
./scripts/release.sh            # next patch version; --minor, --major or an explicit version
```

It stamps `## [Unreleased]` in `CHANGELOG.md` with the version, bumps
`Cargo.toml`, tags and pushes. The `v*` tag runs `.github/workflows/release.yml`,
which builds and signs the three platforms, publishes them with `latest.json`
(read by the in-app update check) and `releases.json` to mayorana.ch, and
creates the GitHub Release. Secrets are the same set as AIS Monitor's.

## Requirements

- Node.js 22.19+, 24.11+ or 26+ (REA's requirement)
- REA: a global `rea` if installed, otherwise `npx rea-agents@6.4.0` is used
- Google Chrome, Chromium or Edge
- Only capture targets you are authorised to test.

## Try it

```bash
cargo run -p surfacewatch-cli -- readiness
```

Capture a target (pages are crawled in batches of 6):

```bash
cargo run -p surfacewatch-cli -- capture http://127.0.0.1:3000 captures/base '#/search' '#/login' '#/basket'
```

Compare two captures, with a second capture of the new build to suppress
unstable endpoints:

```bash
cargo run -p surfacewatch-cli -- diff http://127.0.0.1:3000 captures/base captures/new --repeat captures/new-again
```

Or keep captures per engagement and let `watch` compare each new one with the
first (the baseline):

```bash
cargo run -p surfacewatch-cli -- engagement add acme "Acme" "Retail Q4" --auth "SOW-2026-114" --from 2026-10-01 --to 2026-10-31
cargo run -p surfacewatch-cli -- target add acme shop https://shop.acme.example '#/search' '#/login'
cargo run -p surfacewatch-cli -- watch acme shop --twice
```

The desktop app, where the Watch tab shows the same comparison:

```bash
cargo run
```

## Layout

```
crates/core   surface model, diff engine, REA runner, readiness, store
crates/cli    surfacewatch-cli binary
src/          Dioxus desktop app (screens, components)
```

`crates/core/tests/juice_shop.rs` pins the diff against real REA captures of
OWASP Juice Shop v19.0.0 and v20.3.0.

## Licence

[PolyForm Noncommercial 1.0.0](LICENSE): free for personal use, research and
non-profit organisations. Commercial use, including a consultant using it on
client work, needs a commercial licence: <https://mayorana.ch/en/contact>.
The name and icons are covered by [TRADEMARK.md](TRADEMARK.md).
