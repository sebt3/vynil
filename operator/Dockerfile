ARG RUST_VERSION=1.68
ARG DEBIAN_VERSION=bookworm
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} as builder
ARG BUILD_DEPS="binutils libssl-dev pkg-config git"
WORKDIR /usr/src/operator
# hadolint ignore=DL3008,DL3015
RUN mkdir -p k8s/src operator/src package/src \
 && echo 'fn main() { println!("Dummy") }' > operator/src/main.rs \
 && touch package/src/lib.rs k8s/src/lib.rs operator/src/lib.rs \
 && DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install ${BUILD_DEPS}
COPY operator/parent.toml ./Cargo.toml
COPY src src/
COPY package/Cargo.toml package/Cargo.toml
COPY k8s/Cargo.toml k8s/Cargo.toml
COPY operator/Cargo.toml operator/Cargo.toml
# Build the deps for caching purpose
RUN CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build -j 1 -r --bin operator
# Get our sources
COPY package package/
COPY k8s k8s/
COPY operator operator/
# Update the crates entrypoints so cargo is aware these should rebuild
RUN touch package/src/lib.rs k8s/src/lib.rs operator/src/lib.rs operator/src/main.rs \
 && cargo build -j 1 -r --bin operator \
 && strip target/release/operator

# Finally create the target image with run-time dependencies installed
FROM debian:${DEBIAN_VERSION}-slim as target
ARG DEB_PACKAGES="openssl"
# hadolint ignore=DL3027,DL3008
RUN DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y upgrade \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends ${DEB_PACKAGES} \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /work \
 && chown nobody:nogroup /work
COPY --from=builder /usr/src/operator/target/release/operator /usr/bin/operator
USER nobody
WORKDIR /work
ENTRYPOINT ["operator"]
