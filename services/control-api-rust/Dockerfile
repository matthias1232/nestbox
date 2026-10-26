FROM rust:1.90-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY src src
RUN cargo build --release
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 nestbox
COPY --from=build /app/target/release/nestbox-control-api /usr/local/bin/nestbox-control-api
USER nestbox
EXPOSE 8000
CMD ["nestbox-control-api"]
