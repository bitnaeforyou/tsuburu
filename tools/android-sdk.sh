# Sourced, not run: the Android SDK and the JDK its tools are written in.
#
# Both the build and the publish need these, and neither failure says what is
# wrong. macOS ships a /usr/bin/java whose only purpose is to report that no
# Java is installed, so gradle and apksigner both die with "Unable to locate a
# Java Runtime" while java sits on the path - and Homebrew's JDK is keg-only,
# never linked, which leaves the stub winning.
#
# JDK 21 rather than the newest installed: the Android gradle plugin refuses a
# JDK newer than the one it was tested against.

: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
[ -d "$ANDROID_HOME/platforms" ] || { echo "no Android SDK at $ANDROID_HOME" >&2; exit 1; }
export ANDROID_HOME
export NDK_HOME="${NDK_HOME:-$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"

if [ -z "${JAVA_HOME:-}" ]; then
  JAVA_HOME="$(ls -d /opt/homebrew/Cellar/openjdk@21/*/libexec/openjdk.jdk/Contents/Home 2>/dev/null | sort -V | tail -1)"
  [ -n "$JAVA_HOME" ] || { echo "no JDK 21: brew install openjdk@21" >&2; exit 1; }
fi
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$PATH"
