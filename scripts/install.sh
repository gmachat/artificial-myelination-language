#!/usr/bin/env sh
set -eu

aml_script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
aml_install_dir=${AML_INSTALL_DIR:-"${HOME}/.local/bin"}
aml_destination="${aml_install_dir}/aml"

if [ "${1:-}" = "--uninstall" ]; then
  if [ -f "${aml_destination}" ]; then
    rm "${aml_destination}"
    printf 'Removed %s\n' "${aml_destination}"
  else
    printf 'AML is not installed at %s\n' "${aml_destination}"
  fi
  exit 0
fi

aml_source="${aml_script_dir}/aml"
if [ ! -x "${aml_source}" ]; then
  printf 'AML executable not found beside installer: %s\n' "${aml_source}" >&2
  exit 1
fi

mkdir -p "${aml_install_dir}"
aml_temporary="${aml_destination}.install.$$"
trap 'rm -f "${aml_temporary}"' EXIT HUP INT TERM
cp "${aml_source}" "${aml_temporary}"
chmod 755 "${aml_temporary}"
mv "${aml_temporary}" "${aml_destination}"
trap - EXIT HUP INT TERM

printf 'Installed %s\n' "${aml_destination}"
"${aml_destination}" --version
case ":${PATH}:" in
  *:"${aml_install_dir}":*) ;;
  *) printf 'Add %s to PATH to run aml from any directory.\n' "${aml_install_dir}" ;;
esac

