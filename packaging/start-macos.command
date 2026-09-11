#!/bin/sh
# Double-click this file. If macOS refuses the first time, right-click it and
# choose Open instead — that is the one-time permission for an unsigned app.
#
# It clears the quarantine flag the browser puts on downloads, then starts
# tsuburu. Nothing is installed and nothing outside this folder is touched.
#
# The flag is cleared file by file: xattr has no recursive option on every
# macOS, and the one that does is not the one this has to work on.

cd "$(dirname "$0")" || exit 1
find . -maxdepth 1 -type f -exec xattr -d com.apple.quarantine {} \; 2>/dev/null
chmod +x ./tsuburu 2>/dev/null
exec ./tsuburu
