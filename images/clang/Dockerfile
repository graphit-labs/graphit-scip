FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates curl git build-essential clang cmake ninja-build python3 libtinfo6 && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://github.com/sourcegraph/scip-clang/releases/download/v0.4.0/scip-clang-x86_64-linux -o /usr/local/bin/scip-clang && chmod +x /usr/local/bin/scip-clang
ENV GRAPHIT_SCIP_FAMILY=clang
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
COPY scripts/filter-clang-compdb.py /usr/local/bin/graphit-filter-clang-compdb
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
