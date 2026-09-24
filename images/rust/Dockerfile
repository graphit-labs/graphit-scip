FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git && rm -rf /var/lib/apt/lists/*
RUN rustup component add rust-analyzer
ENV GRAPHIT_SCIP_FAMILY=rust
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
