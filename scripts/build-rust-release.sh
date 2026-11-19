#!/usr/bin/env sh
set -eu

cargo build --release --locked

aml_host="$(rustc -vV | sed -n 's/^host: //p')"

case "${aml_host}" in
  *-apple-darwin) aml_os="macos" ;;
  *-unknown-linux-gnu|*-unknown-linux-musl) aml_os="linux" ;;
  *-pc-windows-*) aml_os="windows" ;;
  *) echo "unsupported Rust host: ${aml_host}" >&2; exit 1 ;;
esac

case "${aml_host}" in
  aarch64-*) aml_arch="arm64" ;;
  x86_64-*) aml_arch="x86_64" ;;
  *) echo "unsupported Rust architecture: ${aml_host}" >&2; exit 1 ;;
esac

aml_name="aml-0.8.0.dev0-rust1-${aml_os}-${aml_arch}"
aml_stage="release/${aml_name}"
mkdir -p "${aml_stage}"

if [ "${aml_os}" = "windows" ]; then
  cp target/release/aml.exe "${aml_stage}/aml.exe"
else
  cp target/release/aml "${aml_stage}/aml"
fi
cp README.md LICENSE scripts/install.sh "${aml_stage}/"
chmod 755 "${aml_stage}/install.sh"

tar -czf "release/${aml_name}.tar.gz" -C release "${aml_name}"
shasum -a 256 "release/${aml_name}.tar.gz" > "release/${aml_name}.tar.gz.sha256"
printf '%s\n' "release/${aml_name}.tar.gz"
