FROM golang:1.26.6-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git && rm -rf /var/lib/apt/lists/*
RUN go install github.com/scip-code/scip-go/cmd/scip-go@v0.2.7 && cp /go/bin/scip-go /usr/local/bin/scip-go
ENV GRAPHIT_SCIP_FAMILY=go
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
