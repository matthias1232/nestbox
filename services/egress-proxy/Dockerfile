# Nestbox egress proxy (Envoy).
#
# The pinned digest is the multi-arch index for `envoyproxy/envoy:v1.31-latest`
# as published on Docker Hub. `latest` in a running system is a promise the
# registry can break; the digest is the same image forever. The readable tag
# stays next to it so a reviewer can tell what is pinned.
#
# Re-pinning is deliberate: pull, read `docker image inspect --format
# '{{index .RepoDigests 0}}'`, update this file, and note it in the changelog.
FROM envoyproxy/envoy:v1.31-latest@sha256:caa5b411be1633b90023592a34a7e010c933d6e60206c758f631485e53006865

# The config and the MCP extension are part of the image, not a bind mount: a
# proxy whose rules come from the host filesystem is a proxy that can be
# changed without a deploy.
COPY envoy.yaml /etc/envoy/envoy.yaml
COPY extensions/ /etc/envoy/extensions/

USER envoy
EXPOSE 3128
CMD ["envoy", "-c", "/etc/envoy/envoy.yaml", "--log-level", "info"]
