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

: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
[ -d "$ANDROID_HOME/platforms" ] || { echo "no Android SDK at $ANDROID_HOME" >&2; exit 1; }
export ANDROID_HOME
export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"

# macOS ships a /usr/bin/java that exists only to say no Java is installed, so
# gradle fails with "Unable to locate a Java Runtime" even though java is on
# the path. Homebrew's JDK is keg-only and never linked, which leaves the stub
# winning. 21 rather than the newest: the Android gradle plugin is pinned to
# what it has been tested against, and a newer JDK is refused outright.
if [ -z "${JAVA_HOME:-}" ]; then
  JAVA_HOME="$(ls -d /opt/homebrew/Cellar/openjdk@21/*/libexec/openjdk.jdk/Contents/Home 2>/dev/null | sort -V | tail -1)"
  [ -n "$JAVA_HOME" ] || { echo "no JDK 21: brew install openjdk@21" >&2; exit 1; }
fi
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$PATH"

# Where it was published from. A build that was not told has no way to update
# itself ever again, and v0.4.1 shipped like that.
export TSUBURU_RELEASES="$repo"

# Absolute source paths are written into the binary, so a package built in a
# home directory carries that home directory - the account name included - to
# everyone who installs it. Taken from the environment rather than written down
# here, so this file names nobody.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$here=/tsuburu"

cd "$here/mobile"
npx tauri android build --target aarch64 --apk
