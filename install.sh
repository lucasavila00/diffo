#!/bin/sh

set -eu

asset="diffo-x86_64-unknown-linux-gnu"
release_url="https://raw.githubusercontent.com/lucasavila00/diffo/release"
install_directory="/usr/local/bin"
destination="${install_directory}/diffo"

fail() {
    printf 'diffo installer: %s\n' "$1" >&2
    exit 1
}

[ "$(uname -s)" = "Linux" ] || fail "only Linux is supported"
[ "$(uname -m)" = "x86_64" ] || fail "only x86_64 is supported"

for required_command in curl sha256sum mktemp install; do
    command -v "${required_command}" >/dev/null 2>&1 \
        || fail "${required_command} is required"
done

temporary_directory=$(mktemp -d)
cleanup() {
    rm -rf "${temporary_directory}"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

printf 'Downloading the latest Diffo release...\n'
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --fail --silent --show-error --location \
    --output "${temporary_directory}/${asset}" "${release_url}/${asset}"
curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --fail --silent --show-error --location \
    --output "${temporary_directory}/SHA256SUMS" "${release_url}/SHA256SUMS"

(
    cd "${temporary_directory}"
    sha256sum --check --ignore-missing SHA256SUMS
) || fail "the downloaded binary did not match SHA256SUMS"

printf 'Installing Diffo to %s...\n' "${destination}"
if [ -w "${install_directory}" ]; then
    install -m 0755 "${temporary_directory}/${asset}" "${destination}"
else
    command -v sudo >/dev/null 2>&1 || fail "sudo is required to install to ${install_directory}"
    sudo install -m 0755 "${temporary_directory}/${asset}" "${destination}"
fi

printf 'Diffo was installed successfully. Run `diffo` from a Git repository.\n'
