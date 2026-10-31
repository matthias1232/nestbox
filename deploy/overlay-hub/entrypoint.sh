#!/bin/sh
# Hub for the node overlay. Files in /overlay:
#   hub.key    the hub's private key, made once, 0600
#   hub.pub    its public key, read by the control plane and given to joining nodes
#   peers.conf the nodes, written by the control plane; applied here within a few seconds
set -eu

DIR=/overlay
IFACE=nb-hub
HUB_IP="${NESTBOX_OVERLAY_HUB_IP:-10.77.0.1}"
PORT="${NESTBOX_OVERLAY_PORT:-51821}"
API_UID="${NESTBOX_API_UID:-10001}"

mkdir -p "$DIR"
umask 077
[ -s "$DIR/hub.key" ] || wg genkey > "$DIR/hub.key"
wg pubkey < "$DIR/hub.key" > "$DIR/hub.pub.new"
chmod 644 "$DIR/hub.pub.new"
mv "$DIR/hub.pub.new" "$DIR/hub.pub"
# The control plane writes peers.conf here as its own user; the key stays root-only.
chown "$API_UID:$API_UID" "$DIR"
chmod 755 "$DIR"
[ -f "$DIR/peers.conf" ] || { : > "$DIR/peers.conf"; chown "$API_UID:$API_UID" "$DIR/peers.conf"; chmod 644 "$DIR/peers.conf"; }

ip link del "$IFACE" 2>/dev/null || true
ip link add "$IFACE" type wireguard
ip addr add "$HUB_IP/16" dev "$IFACE"
wg set "$IFACE" listen-port "$PORT" private-key "$DIR/hub.key"
ip link set "$IFACE" up

# Nodes of different tenants share this interface and must not reach each other: nothing
# is forwarded from the overlay back into the overlay.
iptables -C FORWARD -i "$IFACE" -o "$IFACE" -j DROP 2>/dev/null || iptables -I FORWARD 1 -i "$IFACE" -o "$IFACE" -j DROP
# The control plane's containers reach a node through the host. Masquerade gives their packets
# the hub's address, which is the only address a node accepts.
iptables -t nat -C POSTROUTING -o "$IFACE" -j MASQUERADE 2>/dev/null || iptables -t nat -A POSTROUTING -o "$IFACE" -j MASQUERADE
# Allow the overlay's UDP port in, in case the host filters it.
iptables -C INPUT -p udp --dport "$PORT" -j ACCEPT 2>/dev/null || iptables -I INPUT 1 -p udp --dport "$PORT" -j ACCEPT
# A node reaches the control plane (8080) and the registry (5000) at the hub address only.
iptables -C INPUT -i "$IFACE" -p tcp -m multiport --dports 8080,5000 -j ACCEPT 2>/dev/null \
  || iptables -I INPUT 1 -i "$IFACE" -p tcp -m multiport --dports 8080,5000 -j ACCEPT

# The registry listens on the host's loopback (Docker treats that as insecure-allowed, so the
# host can push without a daemon change); nodes pull it at the hub address.
socat "TCP-LISTEN:5000,bind=$HUB_IP,fork,reuseaddr" TCP:127.0.0.1:5000 &

last=""
while true; do
  if [ -f "$DIR/peers.conf" ]; then
    sum=$(md5sum < "$DIR/peers.conf")
    if [ "$sum" != "$last" ]; then
      {
        printf '[Interface]\nPrivateKey = %s\nListenPort = %s\n' "$(cat "$DIR/hub.key")" "$PORT"
        cat "$DIR/peers.conf"
      } > /tmp/wg.conf
      if wg syncconf "$IFACE" /tmp/wg.conf; then last="$sum"; echo "applied $(grep -c '^\[Peer\]' "$DIR/peers.conf") peer(s)"; fi
    fi
  fi
  sleep 3
done
