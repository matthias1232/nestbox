# The builder: what goes into an image

The builder turns a recipe into an image, and an image into a sandbox. A recipe
is data: a baseline, tools, extra distribution packages, repositories, MCP
servers, agents, resources and mounts. The image is derived from it, tagged by
the content hash of the recipe, so the same recipe always yields the same image
and an edit always yields a new one.

## What the operator can choose

| Choice | Where it comes from | What it becomes |
|---|---|---|
| Baseline | `baselines` rows: Ubuntu 24.04, Debian 12, Rocky 9, Fedora 41, Alpine 3.21 | the `FROM` line and the package manager (`apt`, `dnf`, `apk`) |
| Tool | `tool_catalog`, resolved per baseline | the package name that manager installs |
| Extra package | the distribution's **live** package index | an `apt-get install`/`dnf install`/`apk add` entry |
| Repository | the operator's own HTTPS URL, suite/components and public signing key | a keyring plus a source file written **before** the install |
| MCP server | the MCP registry | an attachment created when the sandbox starts |
| Agent | `agent_packs` | an install step and a supervisor command |

The catalogue is asked, not remembered: `GET /api/v1/packages?baseline=&q=`
runs a container of the baseline image and queries its native package manager.
That is why a package that was released this morning appears without a
deployment, and why an answer of `available: false` means "the index could not
be reached" rather than "nothing matched".

## Why a build is verified

A Dockerfile that ran says the commands succeeded, not that the tool is in the
image. After the build, the runner starts the image and reads its package
database (`dpkg-query`, `rpm -qa`, `apk info`), then compares it with the packages
the recipe asked for. A missing package fails the build with the list, so an
operator learns it from the recipe instead of from an agent that cannot find its
compiler.

## Repositories and credentials

A repository is a public signing key and an HTTPS URL. A private credential is a
`db://` vault locator (ADR 0005), never a value: the database, the manifest and
the generated Dockerfile must not be able to leak one. The keyring is written
into the image, so a package from a private registry installs without a manual
step in the sandbox.

## Templates

A template saves a selection — tools, packages, repositories, MCP servers — for
reuse. It carries choices, never an image: applying it to a recipe copies the
fields, moves the revision and clears `image_ref`, because the previous image
does not contain the new selection. One template per environment may be the
default, and a template may be shared with the environment or kept to its
author.

## Where the code lives

| Question | File |
|---|---|
| How does a recipe become a Dockerfile? | `services/control-api-rust/src/services/builder_service.rs` |
| What is a legal package or repository? | `services/control-api-rust/src/services/selection.rs` |
| How is the catalogue queried? | `services/control-api-rust/src/services/catalogue_service.rs`, `services/sandbox-runner/app/catalogue.py` |
| How do templates share and apply? | `services/control-api-rust/src/services/template_service.rs` |
| Why these decisions? | ADR 0039 |
