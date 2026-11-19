#!/usr/bin/env sh
set -eu

aml_smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/aml-install.XXXXXX")
trap 'rm -rf "${aml_smoke_root}"' EXIT HUP INT TERM
cp target/release/aml scripts/install.sh "${aml_smoke_root}/"
AML_INSTALL_DIR="${aml_smoke_root}/bin" "${aml_smoke_root}/install.sh"
"${aml_smoke_root}/bin/aml" --version
AML_INSTALL_DIR="${aml_smoke_root}/bin" "${aml_smoke_root}/install.sh" --uninstall
test ! -e "${aml_smoke_root}/bin/aml"
printf 'AML install smoke test passed\n'

