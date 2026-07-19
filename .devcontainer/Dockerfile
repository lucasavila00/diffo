FROM mcr.microsoft.com/devcontainers/rust:2-1-trixie

RUN sudo apt-get update \
    && sudo env DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends openssh-server \
    && sudo rm -rf /var/lib/apt/lists/*

RUN curl -LsSf https://insta.rs/install.sh | sh
