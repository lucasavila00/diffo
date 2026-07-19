FROM mcr.microsoft.com/devcontainers/rust:2-1-trixie

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends openssh-server \
    && rm -rf /var/lib/apt/lists/*

RUN curl -LsSf https://insta.rs/install.sh | sh
