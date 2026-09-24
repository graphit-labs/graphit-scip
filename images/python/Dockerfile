FROM node:24-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git python3 python3-pip python3-venv && rm -rf /var/lib/apt/lists/*
RUN npm install -g @sourcegraph/scip-python@0.6.6
ENV GRAPHIT_SCIP_FAMILY=python
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
