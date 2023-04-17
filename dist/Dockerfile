ARG RUST_VERSION=1.68
ARG DEBIAN_VERSION=bookworm
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} as builder
WORKDIR /usr/src/dist
COPY dist/parent.toml ./Cargo.toml
COPY src src/
COPY package/Cargo.toml package/Cargo.toml
COPY dist/Cargo.toml dist/Cargo.toml
# Build the deps for caching purpose
RUN mkdir -p package/src dist/src \
 && echo 'fn main() { println!("Dummy") }' > dist/src/main.rs \
 && touch package/src/lib.rs \
 && cargo build -r --bin dist
# Get our sources
COPY package package/
COPY dist dist/
# Update the crates entrypoints so cargo is aware these should rebuild
RUN touch package/src/lib.rs dist/src/main.rs \
 && cargo build -r --bin dist \
 && strip target/release/dist

# Finally create the target image with run-time dependencies installed
FROM debian:${DEBIAN_VERSION}-slim as target
ARG HELM_VERSION=v3.10.3
ARG KUBECTL_VERSION=v1.25.7
ARG TF_VERSION=1.3.6
ARG DEB_PACKAGES="curl tar gzip unzip openssl ca-certificates"
# hadolint ignore=DL3027,DL3008,DL4006,SC2035
RUN DEBIAN_FRONTEND=noninteractive apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get -y upgrade \
 && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends ${DEB_PACKAGES} \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /work /dist \
 && chown nobody:nogroup /work /dist \
 && case "$(uname -m)" in arm) ARCHITECTURE=arm;; armv8*|aarch64*) ARCHITECTURE=arm64;; x86_64|i686|*) ARCHITECTURE=amd64;; esac \
 && curl -sL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl" -o /usr/local/bin/kubectl \
 && echo "$(curl -sL "https://dl.k8s.io/${KUBECTL_VERSION}/bin/linux/${ARCHITECTURE}/kubectl.sha256") /usr/local/bin/kubectl" | sha256sum --check \
 && curl -sL "https://get.helm.sh/helm-${HELM_VERSION}-linux-${ARCHITECTURE}.tar.gz" |tar --wildcards -C /usr/local/bin/ --strip-components=1 -xzf - */helm \
 && curl -sL "https://releases.hashicorp.com/terraform/${TF_VERSION}/terraform_${TF_VERSION}_linux_${ARCHITECTURE}.zip" -o /tmp/tf.zip \
 && unzip /tmp/tf.zip terraform -d /usr/local/bin \
 && rm /tmp/tf.zip
COPY --from=builder /usr/src/dist/target/release/dist /usr/bin/dist
USER nobody
WORKDIR /work
ENTRYPOINT ["dist"]
