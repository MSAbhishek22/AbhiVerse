# Release procedure

How an Abhiverse version ships. Two kinds of artifact exist and
must not be conflated: a **source release** (tagged tree + zip)
and an **installer release** (built agent binary + APK with
checksums). A source zip alone is not an installable release.

## 1. Prepare

1. Bump the workspace version (`Cargo.toml`), the app version
   (`apps/controller/pubspec.yaml`, `x.y.z+N`), and the README
   status line. Protocol changes also bump `PROTOCOL_VERSION`.
2. Full checks, all must pass:
   `cargo fmt --all --check`, `cargo clippy --workspace
   --all-targets -- -D warnings`, `cargo test --workspace`,
   `cargo check -p abhiverse-win-input -p abhiverse-identity
   --target x86_64-pc-windows-msvc`, `flutter analyze`,
   `flutter test`, and the live E2E suite where the sandbox
   allows it.
3. Update `docs/known-limitations.md` and the changelog notes in
   the commit message; commit in small, reviewable steps.

## 2. Tag and build

```bash
git tag -a vX.Y.Z -m "Abhiverse vX.Y.Z"
git archive --format=zip -o abhiverse-vX.Y.Z.zip vX.Y.Z
```

Installer artifacts (on a Windows machine):

```powershell
cargo build --release -p abhiverse-api -p abhiverse-agent-windows
cd apps\controller; flutter build apk --release
```

Package the agent as a zip containing `abhiverse-agent.exe`,
`abhiverse-api.exe` and a README pointing at
`docs/real-device-setup.md`. Generate checksums:
`Get-FileHash -Algorithm SHA256 *` (or `sha256sum` on Linux).

Signing: binaries are currently unsigned; Windows SmartScreen
will warn. Code signing is a pre-1.0 task — until then, publish
checksums prominently and tell users to expect the warning.

## 3. Publish

1. Sync source to GitHub and read back blob hashes for a sample
   of files (automation does this file-by-file).
2. **Manual (web UI) steps** — GitHub Apps cannot do these:
   - Add/update `.github/workflows/ci.yml` (content lives in the
     repo tree; paste via "Add file" in the web editor).
   - Create the GitHub Release from the tag; attach the source
     zip, the agent zip and the APK with their SHA-256 checksums,
     plus the known-limitations link.
3. Launcher icons are ordinary repo files (all five densities
   are in the tree); they only ever lagged because of text-only
   sync tooling, not because they are special.

## 4. Post-release

- File GitHub issues for anything the validation run surfaced.
- Security reports follow `SECURITY.md` (private reporting, no
  public issues for vulnerabilities).
