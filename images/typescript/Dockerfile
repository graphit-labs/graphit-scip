FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM node:24-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git && rm -rf /var/lib/apt/lists/*
RUN npm install -g @sourcegraph/scip-typescript@0.4.0 corepack && corepack enable
ENV GRAPHIT_SCIP_FAMILY=typescript
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
