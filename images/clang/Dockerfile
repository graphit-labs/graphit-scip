FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin filter-clang-compdb --bin scip-run --bin scip-entrypoint

FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates curl git build-essential clang cmake ninja-build libtinfo6 && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://github.com/sourcegraph/scip-clang/releases/download/v0.4.0/scip-clang-x86_64-linux -o /usr/local/bin/scip-clang && chmod +x /usr/local/bin/scip-clang
ENV GRAPHIT_SCIP_FAMILY=clang
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
COPY --from=graphit-tools /tools/target/release/filter-clang-compdb /usr/local/bin/graphit-filter-clang-compdb
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
