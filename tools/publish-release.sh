#!/usr/bin/env bash
# Finishes a release from this machine.
#
# Two things cannot be done where the rest is built, and both are here for the
# same reason: the keys are on this machine and are not going anywhere else.
# The Android package has to be signed with the key Android will accept, and
# the list of what each file should come to has to be signed with the key the
# update checks against - which is only worth anything as long as whoever
# takes the publishing account cannot sign with it.
#
#   tools/publish-release.sh v0.4.0 owner/repo
#
# Run it after the build that makes everything else has finished, and after
# `npx tauri android build --target aarch64 --apk` in mobile/.
set -euo pipefail

tag="${1:?usage: publish-release.sh <tag> <owner/repo>}"
repo="${2:?usage: publish-release.sh <tag> <owner/repo>}"
version="${tag#v}"
keys="${TSUBURU_SIGNING_DIR:-$HOME/.tsuburu-signing}"
sdk="${ANDROID_HOME:?ANDROID_HOME is not set}"
tools="$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for needed in "$keys/tsuburu.jks" "$keys/password.txt" "$keys/releases.key"; do
  [ -r "$needed" ] || { echo "missing $needed" >&2; exit 1; }
done

# --- the Android package ---

unsigned="$(find mobile/src-tauri/gen/android -name '*-unsigned.apk' -print -quit)"
[ -n "$unsigned" ] || { echo "no built APK; run the android build first" >&2; exit 1; }

apk="$work/tsuburu-$version.apk"
"$tools/zipalign" -p -f 4 "$unsigned" "$work/aligned.apk"
"$tools/apksigner" sign \
  --ks "$keys/tsuburu.jks" --ks-key-alias tsuburu \
  --ks-pass "pass:$(cat "$keys/password.txt")" \
  --key-pass "pass:$(cat "$keys/password.txt")" \
  --out "$apk" "$work/aligned.apk"

# The version in the package, not the version in the name: they came apart
# once, and a rename made it look right.
# No `head` in the pipeline: it leaves early, the writer takes a SIGPIPE,
# and `pipefail` turns that into the whole script stopping here.
said="$("$tools/aapt2" dump badging "$apk" | sed -n "1s/.*versionName='\([^']*\)'.*/\1/p")"
[ "$said" = "$version" ] || { echo "the package says $said, not $version" >&2; exit 1; }
"$tools/apksigner" verify "$apk"
gh release upload "$tag" "$apk" --repo "$repo" --clobber

# --- what the release says its files are, and that it is ours ---

gh release download "$tag" --repo "$repo" -p SHA256SUMS -D "$work"
cd "$work"
# The APK was added after the build wrote this, so its line goes in here.
grep -v " tsuburu-$version.apk\$" SHA256SUMS > sums.new || true
shasum -a 256 "tsuburu-$version.apk" >> sums.new
LC_ALL=C sort -k2 sums.new > SHA256SUMS
rm -f sums.new

openssl pkeyutl -sign -inkey "$keys/releases.key" -rawin -in SHA256SUMS -out SHA256SUMS.sig
openssl pkeyutl -verify -pubin -inkey "$keys/releases.pub" -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig

gh release upload "$tag" SHA256SUMS SHA256SUMS.sig --repo "$repo" --clobber
echo
echo "published tsuburu-$version.apk, and signed what the release says its files are"
