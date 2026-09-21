#!/bin/sh
# Builds an unsigned .ipa for a real iPhone.
#
# Apple will not install an app that nobody signed, and the signature has to
# come from an Apple account - so this deliberately stops one step short and
# hands you the unsigned package. You sign it yourself, with your own Apple ID,
# through AltStore, SideStore or Sideloadly. Nothing here needs an account, a
# paid programme, or anything of ours.
#
#   ./build-ios-unsigned.sh          -> dist/tsuburu-<version>-unsigned.ipa
set -eu

cd "$(dirname "$0")"
here=$(pwd)
version=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
out="$here/dist"

command -v xcodebuild >/dev/null || { echo "Xcode is needed"; exit 1; }
command -v xcodegen  >/dev/null || { echo "xcodegen is needed: brew install xcodegen"; exit 1; }
command -v rustup    >/dev/null || { echo "rustup is needed: https://rustup.rs"; exit 1; }

# Whichever Rust comes first on the PATH is not necessarily rustup's, and only
# rustup's has the iOS target: a Homebrew one builds everything for the Mac and
# then fails on the first crate that needs libc. Ask rustup where it keeps its
# own and put that first.
toolchain=$(rustup which rustc)
PATH="$(dirname "$toolchain"):$PATH"
export PATH
export RUSTC="$toolchain"

echo "==> dependencies"
[ -d node_modules ] || npm install
rustup target add aarch64-apple-ios >/dev/null

echo "==> generating the Xcode project"
# project.yml lists these as source directories, and git does not carry an
# empty one - so a fresh clone fails spec validation before xcodegen starts.
mkdir -p src-tauri/gen/apple/Externals src-tauri/gen/apple/assets
# `ios init` writes project.yml from the Tauri templates, which would take the
# signing settings below with it, so the project is generated from the
# project.yml this repository carries instead.
if [ ! -d src-tauri/gen/apple/tsuburu-mobile.xcodeproj ]; then
  (cd src-tauri/gen/apple && xcodegen generate)
fi

echo "==> building"
app="src-tauri/gen/apple/build/tsuburu-mobile_iOS.xcarchive/Products/Applications/tsuburu.app"
# Cleared first so a build that fails cannot leave the last one behind to be
# packaged as though it were this one.
rm -rf src-tauri/gen/apple/build

# The last step of `tauri ios build` exports a signed package and fails for
# want of a team, which is the point at which this stops. Everything before it
# has to have worked, and the archive it leaves is checked below rather than
# the exit code, which reports that expected failure.
npx tauri ios build --target aarch64 --export-method debugging --ci || true

[ -d "$app" ] || {
  echo
  echo "The build did not produce an app. The output above says why; the"
  echo "\"teamID should be non-empty\" line at the end is expected and is not it."
  exit 1
}
codesign -dv "$app" 2>&1 | grep -q "not signed at all" \
  || echo "note: the app carries a signature; sideloading will replace it"

echo "==> packaging"
rm -rf "$out/Payload" && mkdir -p "$out/Payload"
cp -R "$app" "$out/Payload/"
ipa="$out/tsuburu-$version-unsigned.ipa"
rm -f "$ipa"
(cd "$out" && zip -qry "$(basename "$ipa")" Payload)
rm -rf "$out/Payload"

echo
echo "$ipa"
ls -lh "$ipa" | awk '{print "  " $5}'
echo
echo "Sign it with your own Apple ID:"
echo "  AltStore / SideStore  - on the phone, add the .ipa"
echo "  Sideloadly            - on a computer, with the phone attached"
echo
echo "A free Apple ID lasts seven days before the app stops opening and has to"
echo "be signed again. A paid one lasts a year."
