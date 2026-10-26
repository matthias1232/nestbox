# Build-Stage: kompiliert die API mit allen Abhängigkeiten.
FROM rust:1.90-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# Laufzeit-Stage: nur die Binärdatei, unprivilegierter Benutzer.
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 nestbox
COPY --from=build /app/target/release/nestbox-control-api /usr/local/bin/nestbox-control-api
USER nestbox
EXPOSE 8000
CMD ["nestbox-control-api"]
