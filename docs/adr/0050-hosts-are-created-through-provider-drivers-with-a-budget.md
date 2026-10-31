# ADR 0050 — Hosts are created through provider drivers, with a lifetime and a budget

Status: Accepted
Date: 2026-10-31

## Context

The goal is to start a machine at any cheap host, use it, and delete it, without surprise cost.

## Decision

1. **A `ProviderDriver` trait**: `list_types`, `price`, `create(cloud_init)`, `status`, `destroy`. Credentials are vault locators.
2. **Order**: Proxmox (own infrastructure, first test), NVIDIA Brev, Hetzner, generic SSH; others later. A host without an API uses the cloud-init text or the ISO.
3. **Burst** is one flow: create, join, sync images, move the sandbox, work, move back, dirty check, destroy.
4. **Cost guards**: a lifetime on every created node, stop when idle, a budget per data center, and the hourly price shown before creating.
5. **Distribution**: signed multi-arch images on GHCR, a compose bundle and an installer per release, documentation on GitHub Pages. The hosted service runs the same code on a real server, because Pages is static.

## Alternatives rejected

* **One cloud only.** Defeats "any cheap host".
* **Terraform inside the product.** A second state store beside PostgreSQL.

## Invalidation conditions

Revisited when a provider has no usable API or when billing becomes a product feature.
