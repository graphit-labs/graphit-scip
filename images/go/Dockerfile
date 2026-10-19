FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM golang:1.26.6-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git && rm -rf /var/lib/apt/lists/*
RUN go install github.com/scip-code/scip-go/cmd/scip-go@v0.2.7 && cp /go/bin/scip-go /usr/local/bin/scip-go
ENV GRAPHIT_SCIP_FAMILY=go
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
