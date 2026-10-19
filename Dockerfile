FROM rust:1-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml .
COPY src src
RUN cargo build --release

FROM debian:trixie-slim
ARG RELEASE=dev
ENV RELEASE=${RELEASE}
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3t64 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/sentry-sample-rust /usr/local/bin/sample
EXPOSE 8080
CMD ["sample", "serve"]
