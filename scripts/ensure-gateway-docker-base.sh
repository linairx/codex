#!/usr/bin/env bash
set -euo pipefail

image="codex-alpine:3.24"
url="https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.0-x86_64.tar.gz"
sha256_url="${url}.sha256"

if docker image inspect "${image}" >/dev/null 2>&1; then
  exit 0
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

tarball="${tmp_dir}/alpine-minirootfs.tar.gz"
checksum_file="${tmp_dir}/alpine-minirootfs.tar.gz.sha256"

curl -fsSL "${url}" -o "${tarball}"
curl -fsSL "${sha256_url}" -o "${checksum_file}"

expected_sha256="$(awk '{print $1}' "${checksum_file}")"
printf '%s  %s\n' "${expected_sha256}" "${tarball}" | sha256sum -c -

docker import "${tarball}" "${image}" >/dev/null
