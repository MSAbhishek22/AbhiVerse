# Contributing to ABHIVERSE

1. Fork, branch from `main`, keep changes small and focused.
2. Run the checks before opening a PR:
   - Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
   - Flutter (in `apps/controller`): `flutter analyze`, `flutter test`
3. Test on real devices for anything touching networking,
   permissions or file transfer — unit tests alone are not enough.
4. Never commit secrets, private keys or `.env` files.
