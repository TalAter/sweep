# Linux builds and PTY verification are independent of any sibling checkout.
FROM rust:slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake pkg-config python3 zsh && rm -rf /var/lib/apt/lists/*
WORKDIR /sweep
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM build AS verify
COPY tests ./tests
COPY examples ./examples
# PTY fixtures use the debug-only canned-response seam; separately check release gating.
RUN cargo test --locked && cargo build --locked && python3 tests/terminal.py target/debug/sweep && cargo test --locked --release canned_environment_is_debug_only

FROM ubuntu:24.04 AS sandbox
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl git sudo sqlite3 zsh && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --shell /bin/bash dev && echo 'dev ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/dev
COPY --from=build /sweep/target/release/sweep /usr/local/bin/sweep
USER dev
WORKDIR /home/dev
CMD ["bash"]
