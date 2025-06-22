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
WORKDIR /root/.epic

RUN epic server config && \
    sed -i -e 's/seeding_type = "DNSSeed"/seeding_type = "List"/' epic-server.toml && \
    sed -i '/seeding_type = "List"/a seeds = ["95.217.197.180:3414"]' epic-server.toml && \
    sed -i -e 's/run_tui = true/run_tui = false/' epic-server.toml && \
    sed -i -e 's/stdout_log_level = .*/stdout_log_level = "INFO"/' epic-server.toml

VOLUME ["/root/.epic"]

EXPOSE 3413 3414 3415 3416

ENTRYPOINT ["epic"]
CMD ["server", "run"]
