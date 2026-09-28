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
#
# Run it again after *any* push of the tag, including a forced one. A tag push
# starts the release build over, and that build writes its own SHA256SUMS over
# this one - leaving a list that does not mention the Android package and a
# signature over a file that is no longer there. Every copy out there then
# refuses to update, which is the safe direction but is still broken.
set -euo pipefail

tag="${1:?usage: publish-release.sh <tag> <owner/repo>}"
repo="${2:?usage: publish-release.sh <tag> <owner/repo>}"
version="${tag#v}"
keys="${TSUBURU_SIGNING_DIR:-$HOME/.tsuburu-signing}"
sdk="${ANDROID_HOME:?ANDROID_HOME is not set}"
tools="$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)"
umask 077
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
# `file:` rather than `pass:`: a password on a command line is visible to
# anyone who can run `ps` while this is signing, and to anything that traces
# the script. apksigner reads the two it wants from one stream in order, so
# the same password goes in twice.
pw="$work/pw"
printf '%s\n%s\n' "$(cat "$keys/password.txt")" "$(cat "$keys/password.txt")" > "$pw"
"$tools/apksigner" sign \
  --ks "$keys/tsuburu.jks" --ks-key-alias tsuburu \
  --ks-pass "file:$pw" --key-pass "file:$pw" \
  --out "$apk" "$work/aligned.apk"

# The version in the package, not the version in the name: they came apart
# once, and a rename made it look right.
# No `head` in the pipeline: it leaves early, the writer takes a SIGPIPE,
# and `pipefail` turns that into the whole script stopping here.
said="$("$tools/aapt2" dump badging "$apk" | sed -n "1s/.*versionName='\([^']*\)'.*/\1/p")"
[ "$said" = "$version" ] || { echo "the package says $said, not $version" >&2; exit 1; }

# Where it was published from is baked in at compile time, and a build that
# was not told has no way to ever update itself again. Every other platform
# is built by the workflow, which always sets it; this one is built by hand,
# where it is one forgotten variable away from shipping a dead end. v0.4.1
# went out like that.
unzip -p "$apk" 'lib/arm64-v8a/*.so' > "$work/lib.so"
grep -qa "$repo" "$work/lib.so" || {
  echo "the package does not know where it was published from." >&2
  echo "rebuild it with TSUBURU_RELEASES=$repo set, then run this again." >&2
  exit 1
}
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
