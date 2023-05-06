ARG RUST_VERSION=1.68
ARG DEBIAN_VERSION=bookworm
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} as builder
ARG BUILD_DEPS="binutils libssl-dev pkg-config"
WORKDIR /usr/src/agent
# hadolint ignore=DL3008,DL3015
RUN mkdir -p package/src k8s/src agent/src \
 && echo 'fn main() { println!("Dummy") }' > agent/src/main.rs \
 && touch package/src/lib.rs k8s/src/lib.rs agent/src/lib.rs \
 && DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install ${BUILD_DEPS}
COPY agent/parent.toml ./Cargo.toml
COPY src src/
COPY package/Cargo.toml package/Cargo.toml
COPY k8s/Cargo.toml k8s/Cargo.toml
COPY agent/Cargo.toml agent/Cargo.toml
# Build the deps for caching purpose
RUN cargo build -r --bin agent
# Get our sources
COPY package package/
COPY k8s k8s/
COPY agent agent/
# Update the crates entrypoints so cargo is aware these should rebuild
RUN touch package/src/lib.rs k8s/src/lib.rs agent/src/lib.rs agent/src/main.rs \
 && cargo build -r --bin agent \
 && strip target/release/agent
# Finally create the target image with run-time dependencies installed
FROM debian:${DEBIAN_VERSION}-slim as target
ARG HELM_VERSION=v3.10.3
ARG KUBECTL_VERSION=v1.25.7
ARG TF_VERSION=1.3.6
ARG DEB_PACKAGES="git jq curl tar gzip unzip openssl openssh-client ca-certificates"
# hadolint ignore=DL3008,DL4006,SC2035
RUN DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y upgrade \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends ${DEB_PACKAGES} \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /src /dest /work /nonexistent/.ssh \
 && HOME=/nonexistent git config --global --add safe.directory /work \
 && chown -R nobody:nogroup /src /dest /work /nonexistent \
 && case "$(uname -m)" in arm) ARCHITECTURE=arm;; armv8*|aarch64*) ARCHITECTURE=arm64;; x86_64|i686|*) ARCHITECTURE=amd64;; esac \
 && curl -sL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl" -o /usr/local/bin/kubectl \
 && echo "$(curl -sL "https://dl.k8s.io/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl.sha256") /usr/local/bin/kubectl" | sha256sum --check \
 && curl -sL "https://get.helm.sh/helm-${HELM_VERSION}-linux-${ARCHITECTURE}.tar.gz" |tar --wildcards -C /usr/local/bin/ --strip-components=1 -xzf - */helm \
 && curl -sL "https://releases.hashicorp.com/terraform/${TF_VERSION}/terraform_${TF_VERSION}_linux_${ARCHITECTURE}.zip" -o /tmp/tf.zip \
 && unzip /tmp/tf.zip terraform -d /usr/local/bin \
 && rm /tmp/tf.zip
COPY --from=builder /usr/src/agent/target/release/agent /usr/bin/agent
USER nobody
WORKDIR /work
ENTRYPOINT ["agent"]