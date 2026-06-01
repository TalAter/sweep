# Disposable Linux box for exercising real installers through the locally-built
# sweep (and wrap). Built once, reused; the per-working-copy container mounts
# the host dist/ dirs at /sweep-bin and /wrap-bin.
#
# glibc base on purpose: the bun-compiled binaries are dynamically linked
# against glibc, so musl/alpine would not run them.
FROM ubuntu:24.04

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
       ca-certificates curl git sudo sqlite3 \
  && rm -rf /var/lib/apt/lists/*

# Non-root user with passwordless sudo — mirrors a real dev machine, so
# installers that shell out to `sudo` work and ones that refuse to run as root
# behave faithfully.
RUN useradd --create-home --shell /bin/bash dev \
  && echo 'dev ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/dev \
  && chmod 0440 /etc/sudoers.d/dev

# `sweep`/`wrap`/`w` on PATH point at the linux binaries that will be
# bind-mounted in. Symlink creation does not require the target to exist at
# build time; resolving it does, which the mounts provide at run time. `w`
# (the host alias) deliberately shadows coreutils `w` here.
RUN ln -s /sweep-bin/sweep-linux-arm64 /usr/local/bin/sweep \
  && ln -s /wrap-bin/wrap-linux-arm64  /usr/local/bin/wrap \
  && ln -s /wrap-bin/wrap-linux-arm64  /usr/local/bin/w

USER dev
WORKDIR /home/dev
CMD ["bash"]
