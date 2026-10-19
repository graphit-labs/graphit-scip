FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM eclipse-temurin:25-jdk-noble
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates curl maven git && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://github.com/scip-code/scip-java/releases/download/v0.13.1/scip-java-v0.13.1 -o /usr/local/bin/scip-java && chmod +x /usr/local/bin/scip-java
ENV GRAPHIT_SCIP_FAMILY=java
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
