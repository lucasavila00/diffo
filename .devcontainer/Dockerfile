FROM rust:trixie

RUN apt-get update \
    && env DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends \
        git \
        openssh-server \
        sudo \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash --user-group vscode \
    && echo "vscode ALL=(root) NOPASSWD:ALL" > /etc/sudoers.d/vscode \
    && chmod 0440 /etc/sudoers.d/vscode

RUN rustup update stable \
    && rustup default stable \
    && rustup component add clippy rustfmt

USER vscode

RUN curl -LsSf https://insta.rs/install.sh \
    | env CARGO_HOME=/usr/local/cargo CARGO_INSTA_NO_MODIFY_PATH=1 sh
