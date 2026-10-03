#!/bin/sh
# herdr build step: fetch the prebuilt binary for this version and platform
# from GitHub Releases, check its SHA-256, and fall back to building with
# cargo when there is no release for this platform.
set -eu
cd "$(dirname "$0")/.."
repo="yuya-take/herdr-starkeep"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' herdr-plugin.toml | head -n 1)

build() {
    echo "starkeep: building from source with cargo"
    cargo build --release --locked
    exit $?
}

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-musl ;;
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-musl ;;
    *) build ;;
esac
command -v curl >/dev/null 2>&1 || build

url="https://github.com/$repo/releases/download/v$version/starkeep-$target.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if ! curl -fsSL "$url" -o "$tmp/starkeep.tar.gz" || ! curl -fsSL "$url.sha256" -o "$tmp/starkeep.sha256"; then
    echo "starkeep: no prebuilt binary for v$version on $target"
    build
fi

want=$(cut -d ' ' -f 1 < "$tmp/starkeep.sha256")
if command -v shasum >/dev/null 2>&1; then
    got=$(shasum -a 256 "$tmp/starkeep.tar.gz" | cut -d ' ' -f 1)
else
    got=$(sha256sum "$tmp/starkeep.tar.gz" | cut -d ' ' -f 1)
fi
# A mismatch means a corrupted or tampered download: stop instead of building.
if [ -z "$want" ] || [ "$want" != "$got" ]; then
    echo "starkeep: checksum mismatch for $url" >&2
    exit 1
fi
mkdir -p target/release
tar -xzf "$tmp/starkeep.tar.gz" -C target/release starkeep
chmod 755 target/release/starkeep
echo "starkeep: installed prebuilt v$version for $target"
