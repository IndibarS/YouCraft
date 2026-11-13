#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -x "$project_dir/bin/youcraft" ]; then
    exec "$project_dir/bin/youcraft" "$@"
fi
cd "$project_dir"
exec cargo +1.95.0 run -p filmcraft --bin youcraft -- "$@"
