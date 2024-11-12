ARG RUST_VERSION=1.81
ARG DEBIAN_VERSION=bookworm
ARG DEBIAN_NEXT=trixie
FROM docker.io/rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} AS builder
ARG BUILD_DEPS="binutils libssl-dev pkg-config git build-essential g++"
WORKDIR /usr/src/agent
COPY Cargo.lock .
# hadolint ignore=DL3008,DL3015
RUN mkdir -p common/src agent/src \
 && echo 'fn main() { println!("Dummy") }' > agent/src/main.rs \
 && touch common/src/lib.rs agent/src/lib.rs \
 && DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install ${BUILD_DEPS}
COPY agent/parent.toml ./Cargo.toml
COPY src src/
COPY common/Cargo.toml common/Cargo.toml
COPY agent/Cargo.toml agent/Cargo.toml

# Build the deps for caching purpose
RUN CARGO_NET_GIT_FETCH_WITH_CLI=true cargo build -r --bin agent
# Get our sources
COPY common common/
COPY agent agent/
# Update the crates entrypoints so cargo is aware these should rebuild
RUN touch common/src/lib.rs agent/src/lib.rs agent/src/main.rs \
 && cargo build -r --bin agent \
 && strip target/release/agent
# Then create the intermediary image with run-time dependencies installed
FROM docker.io/debian:${DEBIAN_NEXT}-slim AS middle
ARG HELM_VERSION=v3.10.3
ARG KUBECTL_VERSION=v1.30.3
ARG TF_VERSION=1.8.1
ARG DEB_PACKAGES="git jq curl tar gzip unzip openssl openssh-client ca-certificates postgresql-client mariadb-client-compat restic"
# hadolint ignore=DL3008,DL4006,SC2035
RUN DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y upgrade \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends ${DEB_PACKAGES} \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /package /etc/vynil /work /nonexistent/.ssh \
 && HOME=/nonexistent git config --global --add safe.directory /work \
 && chown -R nobody:nogroup /package /etc/vynil /work /nonexistent \
 && case "$(uname -m)" in arm) ARCHITECTURE=arm;; armv8*|aarch64*) ARCHITECTURE=arm64;; x86_64|i686|*) ARCHITECTURE=amd64;; esac \
 && curl -sL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl" -o /usr/local/bin/kubectl \
 && echo "$(curl -sL "https://dl.k8s.io/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl.sha256") /usr/local/bin/kubectl" | sha256sum --check \
 && curl -sL "https://get.helm.sh/helm-${HELM_VERSION}-linux-${ARCHITECTURE}.tar.gz" |tar --wildcards -C /usr/local/bin/ --strip-components=1 -xzf - */helm \
 && curl -sL "https://github.com/opentofu/opentofu/releases/download/v${TF_VERSION}/tofu_${TF_VERSION}_linux_${ARCHITECTURE}.tar.gz" |tar -C /usr/local/bin/ -xzf - tofu \
 && ln -sf /usr/local/bin/tofu /usr/local/bin/terraform \
 && chmod 755 /usr/local/bin/kubectl /usr/local/bin/helm /usr/local/bin/tofu \
 && mkdir -p /var/lib/vynil/keys /nonexistent \
 && chown -R nobody:nogroup /var/lib/vynil /nonexistent \
 && chmod 770 /var/lib/vynil /nonexistent \
 && chmod 700 /var/lib/vynil/keys
# Use the intermediary image to download the most used providers for runtime caching purpose
FROM middle AS downloader
WORKDIR /src
COPY agent/providers.tf .
RUN tofu init
# Finally assemble everything together
FROM middle AS target
COPY --from=builder /usr/src/agent/target/release/agent /usr/bin/agent
COPY agent/scripts /usr/lib/vynil/scripts
USER nobody
COPY --from=downloader /src/.terraform/providers /nonexistent/.terraform.d/plugins
WORKDIR /work
ENV SCRIPT_DIRECTORY=/usr/lib/vynil/scripts PACKAGE_DIRECTORY=/package CONFIG_DIR=/etc/vynil
ENTRYPOINT ["agent"]
