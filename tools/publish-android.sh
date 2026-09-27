#!/usr/bin/env bash
# Signs the Android package for a tag and adds it to the release.
#
# The build that makes every other file runs where this repository is
# mirrored; this one cannot, because the signing key is on this machine and
# is not going anywhere else. So it is done here, and this script is that
# step written down - including the part that is easy to forget, which is
# that the release names what each of its files should come to and an update
# refuses one that is not named.
#
#   tools/publish-android.sh v0.4.0 owner/repo
set -euo pipefail

tag="${1:?usage: publish-android.sh <tag> <owner/repo>}"
repo="${2:?usage: publish-android.sh <tag> <owner/repo>}"
version="${tag#v}"
keys="${TSUBURU_SIGNING_DIR:-$HOME/.tsuburu-signing}"
sdk="${ANDROID_HOME:?ANDROID_HOME is not set}"
tools="$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

unsigned="$(find mobile/src-tauri/gen/android -name '*-unsigned.apk' | head -1)"
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
said="$("$tools/aapt2" dump badging "$apk" | head -1 | sed -n "s/.*versionName='\([^']*\)'.*/\1/p")"
[ "$said" = "$version" ] || { echo "the package says $said, not $version" >&2; exit 1; }
"$tools/apksigner" verify "$apk"

gh release download "$tag" --repo "$repo" -p SHA256SUMS -D "$work"
gh release upload "$tag" "$apk" --repo "$repo" --clobber

# Added to what the release says its files are, or the update will refuse it.
( cd "$work" && grep -v " tsuburu-$version.apk\$" SHA256SUMS > sums.new || true
  shasum -a 256 "tsuburu-$version.apk" >> sums.new
  mv sums.new SHA256SUMS )
gh release upload "$tag" "$work/SHA256SUMS" --repo "$repo" --clobber
echo "published tsuburu-$version.apk and updated SHA256SUMS"
