# syntax=docker/dockerfile:1

FROM rust:1.91-slim-bookworm AS build
WORKDIR /src
COPY . .
# --locked so the image is built from the committed Cargo.lock and not a fresh resolution.
RUN cargo build --release --locked --bin watcher

FROM debian:bookworm-slim AS runtime
# The price, news and model providers are all HTTPS, so the runtime needs root certificates.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN useradd --system --no-create-home --uid 10001 watcher
COPY --from=build /src/target/release/watcher /usr/local/bin/watcher
USER watcher
EXPOSE 8080
# Exec form, so the process is PID 1 and receives SIGTERM instead of being killed after the
# shutdown grace period.
ENTRYPOINT ["/usr/local/bin/watcher"]
