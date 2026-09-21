# tsuburu on a phone

The same program, in a window the operating system gave it rather than one a
browser did. The Rust core and the Svelte interface are the ones the desktop
build uses — nothing here is a second copy of either.

## How it fits together

The server runs *inside* the app. On start it opens the same stores the
desktop program opens, binds a loopback port the system picks, and the webview
is pointed at it. The interface already speaks to `/api/...` over HTTP and
already knows how to be held in one hand, so it needed no changes at all.

Nothing is talking to a machine somewhere else. The phone is the machine, the
port is on the loopback address, and nothing outside the app can reach it.

```
  ┌─────────────────── the app ───────────────────┐
  │  webview  ──http://127.0.0.1:{picked}──▶  axum │
  │  (web/dist, as served)                    │    │
  │                                    tsuburu-server
  │                            stores · sweep · OCR │
  └───────────────────────────────────────────────┘
```

Opening the stores, resuming the model and starting the sweep is
`tsuburu_server::boot::assemble`, which the desktop program calls too — the two
cannot drift.

## iOS

Built and run on a simulator:

```console
$ npm install
$ npx tauri ios build --debug --target aarch64-sim
$ xcrun simctl install booted src-tauri/gen/apple/build/arm64-sim/tsuburu.app
$ xcrun simctl launch booted la.tsuburu.tsuburu
```

`npx tauri ios dev` does the same thing with a live reload attached.

Recognition uses Vision, the same framework the macOS build uses — an iPhone
reads a page without being sent anywhere for it.

A build for a device rather than a simulator needs a signing identity, which
is an Apple developer account and not something this repository carries.

## Android

```console
$ npm install
$ npx tauri android build --debug --target aarch64
$ adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
$ adb shell am start -n la.tsuburu.tsuburu.debug/la.tsuburu.tsuburu.MainActivity
```

The debug package is `la.tsuburu.tsuburu.debug`, so a debug build and a
release one can sit on the same phone.

It needs a JDK and the SDK, neither of which comes with the repository:

```console
$ brew install openjdk@21
$ brew install --cask android-commandlinetools
$ sdkmanager "platform-tools" "platforms;android-35" \
    "build-tools;35.0.0" "ndk;27.1.12297006"
$ rustup target add aarch64-linux-android armv7-linux-androideabi \
    i686-linux-android x86_64-linux-android
```

and `JAVA_HOME`, `ANDROID_HOME` and `NDK_HOME` set to where those landed.

There is no OCR on Android. `platform_ocr` has a backend for Vision and one
for Windows and one for tesseract, and Android is none of those, so dialogue
recognition reports itself unsupported and everything else works. Reading what
somebody else has already recognised - an imported index - is unaffected.

### Signing an Android release

Android will not install an APK with no signature at all, but it does not care
who signed it: a key you made yourself is what every sideloaded app carries.
The keystore is not in this repository - losing it means the next version
cannot replace this one on a phone that has it, so keep a copy somewhere safe.

Making one, once:

```console
$ keytool -genkeypair -v -keystore tsuburu.jks -alias tsuburu \
    -keyalg RSA -keysize 4096 -validity 10950
```

Then, for each release:

```console
$ npx tauri android build --target aarch64 --apk
$ zipalign -p -f 4 <the unsigned apk> aligned.apk
$ apksigner sign --ks tsuburu.jks --ks-key-alias tsuburu \
    --out tsuburu-<version>.apk aligned.apk
$ apksigner verify --print-certs tsuburu-<version>.apk
```

A phone will still warn once, because the key is not one Google knows: allow
installing from the browser or file manager that handed it over. That is the
same warning every sideloaded app produces and is not a property of this one.

### Signing an iOS build

There is no equivalent. Apple does not install an unsigned app at all, and the
signature has to come from an Apple account:

- **A simulator** needs nothing, which is what the build above uses.
- **Your own phone** needs an Apple ID signed into Xcode. A free one works and
  the app stops running after seven days; a paid one lasts a year. Set
  `developmentTeam` under `bundle > iOS` in `tauri.conf.json` - it is left out
  here because it names whoever built it.
- **Anybody else's phone** needs the paid Apple Developer Program, and either
  ad-hoc distribution against device identifiers collected in advance, or the
  App Store, which this would not pass.

So the package this repository builds is deliberately unsigned, and whoever
wants it signs it themselves:

```console
$ ./build-ios-unsigned.sh          # -> dist/tsuburu-<version>-unsigned.ipa
```

Then AltStore or SideStore on the phone, or Sideloadly from a computer, signs
it with *your* Apple ID. Nothing of ours is in that signature and no account
of ours is involved. A free Apple ID lasts seven days before the app stops
opening and has to be signed again; a paid one lasts a year.

The script stops one step short of what `tauri ios build` would do: the final
export is what needs a team, so it is allowed to fail and the app is taken
from the archive the build already wrote. It clears the previous build first,
so a failure cannot leave the last one behind to be packaged as if it were
this one.

## Sizes

A release APK is about 14 MB. The debug one is forty times that, because it
keeps every architecture and every symbol; that is a debug build being a debug
build, not something to fix.

## What is generated

`src-tauri/gen/android` is not committed at all: it is a Gradle project
generated from the config, with a copy of the Gradle wrapper in it.

`src-tauri/gen/apple` holds the Xcode project's *sources* — the plist, the
entitlements, the launch screen, `project.yml`. The `.xcodeproj` itself is
generated from that by xcodegen on every `tauri ios` command and is not
committed, because it carries absolute paths from whichever machine ran it.
