#!/usr/bin/env bash
# Builds the Android package, which is the one artifact not built by the
# workflow: it has to be signed with the key that lives on this machine.
#
#   tools/build-apk.sh owner/repo
#
# Four things have to be right and none of them are visible in the result,
# which is why they are here rather than in somebody's shell history.
set -euo pipefail

repo="${1:?usage: build-apk.sh <owner/repo>}"
here="$(cd "$(dirname "$0")/.." && pwd)"

# Homebrew's rustc carries no Android standard library, and when it is first on
# the path the build fails a long way in with `can't find crate for core`.
export PATH="$HOME/.cargo/bin:$PATH"

# shellcheck source=tools/android-sdk.sh
. "$here/tools/android-sdk.sh"

# Where it was published from. A build that was not told has no way to update
# itself ever again, and v0.4.1 shipped like that.
export TSUBURU_RELEASES="$repo"

# Absolute source paths are written into the binary, so a package built in a
# home directory carries that home directory - the account name included - to
# everyone who installs it. Taken from the environment rather than written down
# here, so this file names nobody.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$here=/tsuburu"

# The interface is embedded into the binary from `web/dist` as it compiles,
# and nothing in the Android build produces it. Every other platform is built
# by the workflow, which builds the frontend as its own step; this one is
# built by hand, where whatever `web/dist` happened to hold is what ships.
# v0.4.9 through v0.6.2 each went out carrying the interface from v0.4.8.
cd "$here/web"
npm ci
npm run build

cd "$here/mobile"
npx tauri android build --target aarch64 --apk
