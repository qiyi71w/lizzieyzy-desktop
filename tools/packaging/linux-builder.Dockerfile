# Linux package builder. Ubuntu 24.04 is the oldest supported Linux, so packages built here
# depend on its glibc and WebKitGTK and also run on newer releases. Used by build-linux.sh.
FROM ubuntu:24.04

ARG RUST_VERSION=1.98.0
ARG NODE_VERSION=22.23.2

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/node/bin:$PATH \
    APPIMAGE_EXTRACT_AND_RUN=1

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl file git pkg-config xz-utils \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}"

RUN set -eux; \
    file="node-v${NODE_VERSION}-linux-x64.tar.xz"; \
    curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/${file}"; \
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" | grep " ${file}\$" | sha256sum -c -; \
    mkdir /usr/local/node; \
    tar -xJf "${file}" -C /usr/local/node --strip-components=1; \
    rm "${file}"
