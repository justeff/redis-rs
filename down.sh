#!/bin/sh
# Stop everything up.sh started (both modes).
cd "$(dirname "$0")"
podman rm -f rrb-client rrb-redis >/dev/null 2>&1
if [ -f redis-native.pid ]; then kill "$(cat redis-native.pid)" 2>/dev/null; rm -f redis-native.pid; fi
exit 0
