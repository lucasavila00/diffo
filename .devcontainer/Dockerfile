FROM mcr.microsoft.com/devcontainers/rust:2-1-trixie

RUN sudo apt-get update \
    && sudo env DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends openssh-server \
    && sudo rm -rf /var/lib/apt/lists/*

USER vscode

RUN curl -LsSf https://insta.rs/install.sh \
    | env CARGO_HOME=/usr/local/cargo CARGO_INSTA_NO_MODIFY_PATH=1 sh
