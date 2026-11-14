# syntax=docker/dockerfile:1
# endora-foundation: the foundation's command line as a small container image.
# Build from the repository root:  docker build -f deploy/foundation/Dockerfile -t endora-foundation .
# Built and run on the Linux test host (14 Nov 2026): non-root, healthcheck passes, `serve` exits on SIGTERM.
# Both base images are pinned by digest (the tag is kept for people; the digest is what is used).
# The image has no git and no sandbox: `git-task` is unavailable in it (see README.md).

# --- build stage -----------------------------------------------------------
# Pinned to the workspace MSRV. rusqlite bundles SQLite, so the builder's C toolchain is all it
# needs and the runtime needs no SQLite library.
FROM rust:1.87-bookworm@sha256:251cec8da4689d180f124ef00024c2f83f79d9bf984e43c180a598119e326b84 AS builder
ARG CARGO_JOBS=4
ENV RUSTUP_TOOLCHAIN=1.87.0
WORKDIR /src
COPY . .
RUN cargo build --release -p endora-foundation-cli --locked -j "${CARGO_JOBS}" \
 && strip target/release/endora-foundation

# --- runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
# A non-root user owns /data, the only place the program writes.
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin foundation \
 && mkdir -p /data /config && chown foundation:foundation /data
COPY --from=builder /src/target/release/endora-foundation /usr/local/bin/endora-foundation
ENV ENDORA_FOUNDATION_DB=/data/foundation.db \
    ENDORA_FOUNDATION_CONFIG=/config/foundation.conf
USER foundation
VOLUME ["/data"]
HEALTHCHECK --interval=60s --timeout=10s --start-period=15s --retries=3 \
  CMD ["endora-foundation", "status"]
ENTRYPOINT ["endora-foundation"]
# Run due work every minute; SIGTERM ends the loop cleanly.
CMD ["serve", "--every", "60"]
