# OpenChamber (the web front end) and OpenCode (the agent it drives).
#
# Both are pinned by argument, not `latest`. The web UI listens on 3000 and the
# control plane publishes that port on the node; `entrypoint.sh` makes the UI
# password-protected before it does.

FROM node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ARG OPENCHAMBER_VERSION=2.5.3
ARG OPENCODE_VERSION=1.19.5

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git jq less procps htop ripgrep vim openssh-client \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash nestbox
USER nestbox
WORKDIR /home/nestbox

ENV NPM_CONFIG_PREFIX=/home/nestbox/.local
ENV PATH=/home/nestbox/.local/bin:${PATH}

RUN npm install -g "@openchamber/web@${OPENCHAMBER_VERSION}" "opencode-ai@${OPENCODE_VERSION}" \
    && openchamber --version \
    && opencode --version

COPY --chown=nestbox:nestbox entrypoint.sh /home/nestbox/.local/bin/nestbox-openchamber

EXPOSE 3000
CMD ["/home/nestbox/.local/bin/nestbox-openchamber"]
