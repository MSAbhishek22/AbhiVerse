# Contributing to Abhiverse

Contributions are welcome — bug reports, fixes, docs, and
real-device test results (see `docs/real-device-test.md`; a
filled-in matrix from hardware we don't have is one of the most
valuable contributions right now).

## Getting set up

- Rust (stable, via rustup) and Flutter (stable) — that's all.
- Build everything: `cargo build --workspace` and, in
  `apps/controller`, `flutter pub get`.
- Run the API + agent locally with `docs/real-device-setup.md`.

## Before opening a PR

Run the full check suite and make sure it's green:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo check -p abhiverse-win-input -p abhiverse-identity --target x86_64-pc-windows-msvc`
  (Windows-only crates must keep compiling for Windows)
- In `apps/controller`: `flutter analyze` and `flutter test`

Keep changes small and focused, one topic per PR, with tests for
new behaviour. Anything touching pairing, signatures, session
authorisation, file paths or input injection needs a security
test as well — `server/api/tests/security.rs` shows the pattern.

## Ground rules

- Never commit secrets, private keys, or `.env` files.
- Don't weaken a security check to make a test pass; if a check
  looks wrong, open an issue first.
- Remote input stays opt-in (`--allow-input`) and there is no
  remote shell — PRs adding either will be declined.
- Test on real devices for anything touching networking,
  permissions or file transfer; unit tests alone are not enough
  evidence for those paths.

## Reporting bugs

Use the issue templates. For anything exploitable, follow
`SECURITY.md` instead of filing a public issue.
