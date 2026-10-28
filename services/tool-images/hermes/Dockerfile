# Hermes Agent on Ubuntu 24.04.
#
# The installer is pinned by the version it reports at build time rather than by
# a floating tag: the image tag `nestbox-hermes:local` is the pin the control
# plane records, and the build log holds the resolved version.
#
# The model default is baked in here so a freshly created sandbox answers a plain
# `hermes -z "..."` without an operator running `hermes setup` first. The
# endpoint and the credential are NOT baked in: both arrive as environment
# variables from the provider profile at container start, so a rotated key does
# not require an image rebuild (ADR 0005).

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git python3 nodejs npm jq less procps htop ripgrep vim \
        libatomic1 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash nestbox
USER nestbox
WORKDIR /home/nestbox

RUN curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash

# Pin the inference provider and model to what the appliance serves. A bare
# `hermes -z` otherwise falls back to the vendor's default model, which the local
# key is not allowed to use, and the failure looks like a broken agent rather
# than a wrong default.
RUN /home/nestbox/.local/bin/hermes config set model.default opencode-go-combo \
    && /home/nestbox/.local/bin/hermes config set model.provider openai \
    && /home/nestbox/.local/bin/hermes config check || true

# A sandbox is a long-lived container, not a one-shot command.
CMD ["sleep", "infinity"]
