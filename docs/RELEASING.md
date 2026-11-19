# Releasing AML

AML's primary distribution is one native Rust executable. Users do not need
Python, Rust, or repository dependencies after downloading a platform archive.

## Before a release

1. Update the version in `Cargo.toml`, `src/lib.rs`, and the archive script.
2. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
   `cargo test`.
3. Run `python3 -m unittest discover -s tests` for legacy parity.
4. Run `node scripts/conformance.mjs` on a machine with target toolchains.
5. Review `aml benchmark --json` and publish it only as an authored-source
   measurement.

Build a local native archive:

```sh
./scripts/build-rust-release.sh
```

Tag pushes trigger `.github/workflows/release.yml`. Native GitHub runners build
and test Linux x86-64, macOS x86-64, macOS ARM64, and Windows x86-64 archives.
The publish job creates SHA-256 checksums. Public macOS artifacts should also be
Developer ID signed and notarized before AML is described as production-ready.

## Test an archive

Extract it in a clean directory and run the embedded compiler:

```sh
tar -xzf release/aml-*.tar.gz
./aml-*/aml --version
./aml-*/aml reference
```

Also smoke-test installation into a temporary directory:

```sh
./scripts/smoke-install.sh
```

## Publish

After all CI jobs pass, create and push a version tag:

```sh
git tag v0.8.0
git push origin v0.8.0
```

Publishing is an external action and should happen only after project ownership,
license, version, signing policy, and repository security settings are confirmed.
