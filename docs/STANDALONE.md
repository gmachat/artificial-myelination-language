# Standalone AML executable

AML can be distributed as a single platform-specific executable. End users do
not need Python or third-party Python packages. The Python interpreter and AML
standard toolchain are bundled inside the executable.

The standalone build dependency is PyInstaller. It is optional and does not
become an AML runtime dependency.

## Local build

```sh
python3 -m venv .venv-standalone
. .venv-standalone/bin/activate
python -m pip install ".[standalone]"
python scripts/build_standalone.py
```

The script creates a versioned directory, SHA-256 checksum, and `.tar.gz` or
`.zip` archive under `release/`. It smoke-tests version reporting, source
checking, and `aml run` before producing the checksum.

The local macOS artifact is ad-hoc signed by the build tooling but is not
notarized with an Apple Developer account. Public macOS releases should be
Developer ID signed and notarized before distribution.

## Cross-platform releases

`.github/workflows/standalone.yml` builds separate macOS arm64, Linux x86-64,
and Windows x86-64 artifacts on version tags or manual dispatch. Native
executables must be built on their target operating system; they are not one
universal file.

Generated programs still require the selected target platform when run through
AML—for example, `--target rust` requires a Rust compiler. The default Python
target runs inside the bundled executable and does not call external Python.
