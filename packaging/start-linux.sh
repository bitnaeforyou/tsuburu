#!/bin/sh
# Run this file. Nothing is installed; tsuburu stays in this folder.
cd "$(dirname "$0")" || exit 1
chmod +x ./tsuburu 2>/dev/null
exec ./tsuburu
