# Claude Code in a sandbox, for SSH: `ssh -t ... nestbox@node claude`.
#
# The version is pinned by an argument rather than `latest`: the tag
# `nestbox-claude-code:local` is what the control plane records, and the build log
# holds the resolved version. The credential is NOT baked in: ANTHROPIC_API_KEY
# arrives in the sandbox environment, or the operator signs in on first run.
#
# Runs as `nestbox`, the user the SSH door and the agents use, so a login lands in
# the home the agent works in.

FROM node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ARG CLAUDE_CODE_VERSION=2.1.312

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git jq less procps htop ripgrep vim openssh-client \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash nestbox
USER nestbox
WORKDIR /home/nestbox

# A user-level prefix: no root needed at run time, and `docker exec` and the SSH
# door find the command without a login shell reading ~/.bashrc.
ENV NPM_CONFIG_PREFIX=/home/nestbox/.local
ENV PATH=/home/nestbox/.local/bin:${PATH}

RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && claude --version

CMD ["sleep", "infinity"]
