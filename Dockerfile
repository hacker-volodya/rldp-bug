FROM rust:1.77.2-buster
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/var/cache/buildkit \
    CARGO_HOME=/var/cache/buildkit/cargo \
    CARGO_TARGET_DIR=/var/cache/buildkit/target \
    cargo build --release --locked && \
    cp /var/cache/buildkit/target/release/rldp-bug /
CMD /rldp-bug