ARG RUST_IMAGE=rust:1.91-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim
ARG DEBIAN_MIRROR=mirrors.aliyun.com
ARG CARGO_REGISTRY=sparse+https://rsproxy.cn/index/

FROM ${RUST_IMAGE} AS build
ARG IROH_RELAY_VERSION=1.0.0-rc.1
ARG DEBIAN_MIRROR
ARG CARGO_REGISTRY

RUN printf 'Acquire::ForceIPv4 "true";\nAcquire::Retries "5";\nAcquire::http::Timeout "30";\n' > /etc/apt/apt.conf.d/99network \
 && sed -i "s|deb.debian.org|${DEBIAN_MIRROR}|g" /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates clang git lld pkg-config \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
RUN mkdir --parents "${CARGO_HOME}" \
 && printf '[source.crates-io]\nreplace-with = "build-mirror"\n\n[source.build-mirror]\nregistry = "%s"\n\n[net]\nretry = 5\ngit-fetch-with-cli = true\n' "${CARGO_REGISTRY}" > "${CARGO_HOME}/config.toml" \
 && git clone --depth 1 --branch "v${IROH_RELAY_VERSION}" https://github.com/n0-computer/iroh.git . \
 && cargo build --release --features=server --bin iroh-relay

FROM ${RUNTIME_IMAGE}
ARG DEBIAN_MIRROR

RUN printf 'Acquire::ForceIPv4 "true";\nAcquire::Retries "5";\nAcquire::http::Timeout "30";\n' > /etc/apt/apt.conf.d/99network \
 && sed -i "s|deb.debian.org|${DEBIAN_MIRROR}|g" /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && apt-get install --yes --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --home-dir /var/lib/iroh-relay iroh-relay

COPY --from=build /src/target/release/iroh-relay /usr/local/bin/iroh-relay

USER iroh-relay
EXPOSE 80/tcp 443/tcp 7842/udp
ENTRYPOINT ["iroh-relay"]
