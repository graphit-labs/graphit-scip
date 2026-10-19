FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM rust:1-bookworm
RUN rustup component add rust-analyzer rust-src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libopenblas-pthread-dev libgfortran-12-dev && rm -rf /var/lib/apt/lists/*
ENV GRAPHIT_SCIP_FAMILY=rust
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
