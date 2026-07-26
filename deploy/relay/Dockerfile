ARG RUST_IMAGE=rust:1.85-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim

FROM ${RUST_IMAGE} AS build
ARG IROH_RELAY_VERSION=1.0.0-rc.1

RUN apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates git pkg-config \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
RUN git clone --depth 1 --branch "v${IROH_RELAY_VERSION}" https://github.com/n0-computer/iroh.git . \
 && cargo build --release --features=server --bin iroh-relay

FROM ${RUNTIME_IMAGE}

RUN apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --home-dir /var/lib/iroh-relay iroh-relay

COPY --from=build /src/target/release/iroh-relay /usr/local/bin/iroh-relay

USER iroh-relay
EXPOSE 80/tcp 443/tcp 7842/udp
ENTRYPOINT ["iroh-relay"]
