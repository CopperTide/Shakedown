#!/bin/sh
# Relays 127.0.0.1:3128 to the recording proxy's socket, waits until it listens, then runs the
# command. 3128 is 0C38 in /proc/net/tcp; reading it opens no connection to the proxy.
set -eu
socat TCP-LISTEN:3128,bind=127.0.0.1,fork,reuseaddr UNIX-CONNECT:/run/arena-proxy.sock &
i=0
until grep -q ':0C38 00000000:0000 0A' /proc/net/tcp; do
    i=$((i + 1))
    [ "$i" -le 200 ] || { echo "bridge-then: the proxy relay did not start" >&2; exit 125; }
    sleep 0.01
done
exec "$@"
