#!/bin/sh
# Install the desktop pack: a VNC server, a light desktop, Google Chrome, and the
# Chrome DevTools MCP server that lets an agent drive that Chrome.
#
# Run once at image build time, as root. Ubuntu/Debian only (apt): the pack's
# `base_os` says so, and the builder refuses it on another baseline.
#
# XFCE rather than GNOME: GNOME needs systemd and a session bus the container
# does not have, and costs about twice the memory for the same use.
set -eu
export DEBIAN_FRONTEND=noninteractive

apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl gnupg python3 \
  tigervnc-standalone-server tigervnc-common \
  xfce4 xfce4-terminal dbus-x11 x11-utils xauth xdg-utils \
  libnss3-tools \
  fonts-dejavu-core fonts-liberation fonts-noto-color-emoji \
  nodejs npm

# Google Chrome comes from Google's own repository: Ubuntu's chromium is a snap,
# and a snap does not run in a container.
curl -fsSL --proto '=https' --tlsv1.2 https://dl.google.com/linux/linux_signing_key.pub \
  | gpg --dearmor -o /usr/share/keyrings/google-chrome.gpg
echo "deb [arch=amd64 signed-by=/usr/share/keyrings/google-chrome.gpg] https://dl.google.com/linux/chrome/deb/ stable main" \
  > /etc/apt/sources.list.d/google-chrome.list
apt-get update
apt-get install -y --no-install-recommends google-chrome-stable

# The MCP server agents use to open pages, click, type and take screenshots. It
# attaches to the Chrome that `nestbox-desktop` starts (remote debugging on
# 127.0.0.1:9222), so what the agent does is visible on the desktop.
npm install --global chrome-devtools-mcp@latest
npm cache clean --force

rm -rf /var/lib/apt/lists/*
