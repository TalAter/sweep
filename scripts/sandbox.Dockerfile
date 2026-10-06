# Linux builds and PTY verification are independent of any sibling checkout.
FROM rust:slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake pkg-config python3 zsh && rm -rf /var/lib/apt/lists/*
WORKDIR /sweep
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM build AS verify
COPY tests ./tests
RUN cargo test --locked && python3 tests/terminal.py target/release/sweep

FROM ubuntu:24.04 AS sandbox
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git sudo sqlite3 zsh && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --shell /bin/bash dev && echo 'dev ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/dev
COPY --from=build /sweep/target/release/sweep /usr/local/bin/sweep
USER dev
WORKDIR /home/dev
CMD ["bash"]
