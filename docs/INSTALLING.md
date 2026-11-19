# Installing AML

AML is distributed as a platform archive containing one native compiler and a
small installer. The compiler has no runtime dependency on Python or Rust.

## macOS and Linux

Extract the archive, enter its directory, and run:

```sh
./install.sh
aml --version
aml doctor
```

By default this installs to `$HOME/.local/bin/aml`. Set `AML_INSTALL_DIR` for a
different location. Uninstall from the extracted archive with:

```sh
./install.sh --uninstall
```

## Windows

Extract the ZIP and run PowerShell from its directory:

```powershell
.\install.ps1
aml --version
aml doctor
```

The default destination is `%LOCALAPPDATA%\AML\bin\aml.exe`. A custom location
and uninstall are explicit:

```powershell
.\install.ps1 -InstallDir "$HOME\bin"
.\install.ps1 -InstallDir "$HOME\bin" -Uninstall
```

## What `aml doctor` means

`aml doctor` reports whether each target's external tool can be found. AML can
still generate Java source without a local JDK, for example. Discovery does not
prove a toolchain is internally healthy; maintainers use
`scripts/conformance.mjs` for execution certification.

