# Multistage Docker build for Epic mainnet (latest Rust & Linux)

# ---- Builder Stage ----
FROM rust:latest AS builder

RUN apt-get update && \
    apt-get install --no-install-recommends -y \
        clang \
        libclang-dev \
        llvm-dev \
        libncurses5 \
        libncursesw5 \
        cmake \
        git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/epic

COPY . .

RUN cargo build --release

# ---- Runtime Stage ----
FROM debian:stable-slim

RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        locales \
        openssl \
        libncursesw6 \
    && rm -rf /var/lib/apt/lists/*

RUN sed -i -e 's/# en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen && \
    locale-gen

ENV LANG=en_US.UTF-8

COPY --from=builder /usr/src/epic/target/release/epic /usr/local/bin/epic
COPY debian/foundation.json /usr/share/epic/foundation.json

WORKDIR /root/.epic/main/
# create a epic-server.toml file with default values
RUN epic server config 


EXPOSE 3413 3414 3415 3416


CMD ["epic", "server", "-c", "/root/.epic/main/epic-server.toml", "run"]

