# Build the API, then ship only the binary.
#
# The migrations are copied in because `sqlx::migrate!` embeds them at compile
# time: the running container needs no migrations directory and no database
# access at build.

FROM rust:1.90-bookworm AS build
WORKDIR /app

# Dependencies first, so a source-only change does not re-download the world.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY migrations ./migrations
COPY src ./src
# Touch main.rs so cargo rebuilds the crate after the dependency cache pass.
RUN touch src/main.rs && cargo build --release --locked

FROM debian:bookworm-slim

# ca-certificates for outbound TLS to gateways and providers; nothing else.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home nestbox

COPY --from=build /app/target/release/nestbox-control-api /usr/local/bin/nestbox-control-api

# Unprivileged. The API writes nothing to disk; all state is in PostgreSQL.
USER 10001:10001
EXPOSE 8000
CMD ["nestbox-control-api"]
