FROM dart:stable
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git && rm -rf /var/lib/apt/lists/*
RUN dart pub global activate scip_dart 1.6.2
ENV GRAPHIT_SCIP_FAMILY=dart
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
