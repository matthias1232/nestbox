FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git nano jq less procps iputils-ping dnsutils ripgrep vim && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
CMD ["sleep", "infinity"]

## desktop/

`install.sh` and `nestbox-desktop` are the scripts of the `desktop` agent pack
(install kind `desktop`). They are embedded in the generated Dockerfile and
hashed into the image tag, so editing one produces a new image. The desktop is
Xtigervnc + XFCE + Google Chrome (remote debugging on 127.0.0.1:9222) and the
Chrome DevTools MCP server; nothing listens outside the container.

## claude-code/

`Dockerfile` builds the image whose agents sign in themselves: Claude Code plus the
OpenAI Codex CLI, each pinned by an `ARG`, so one sandbox can switch between them.
