# SurfaceWatch

Modelled on AIS Monitor: a Dioxus desktop app at the root, a shared
`crates/core`, and a headless edition (`crates/cli`) instead of a TUI. REA is
driven as a subprocess through its documented CLI, the way AIS Monitor drives
`az`.

## Design rules

- REA is one source behind an adapter, not the foundation. Its own
  comparison and static JS extraction were not usable in the Juice Shop spike;
  the surface model and diff engine in `crates/core` are this app's value.
- Raw REA Evidence is stored unchanged. Its IDs are content hashes that
  findings cite. Surfaces and diffs are derived and rebuildable.
- Read REA output leniently: accept the Evidence envelope or its
  `normalized_result`, ignore unknown fields.
- Persist through `store::write_private` (atomic, 0600).
- Refuse captures outside an engagement's test window.

## Release notes

Every change a user could notice gets a bullet in `CHANGELOG.md` under
`## [Unreleased]` (`### Added`, `### Changed`, `### Fixed`, `### Removed`), in
the same change. Write for the user, not the reviewer: no module or function
names. Never write a version heading yourself.

## Checks

Do not run `dx fmt`: it has dropped `else` branches from `if … else` inside
`rsx!`. `cargo fmt` leaves `rsx!` alone; format markup by hand.


The compiler is pinned in `rust-toolchain.toml` (rustup installs it on first
use); the workflows install that version with `rustup toolchain install`. Bump
it in every desktop app at once.

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets
```
