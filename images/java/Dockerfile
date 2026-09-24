FROM eclipse-temurin:25-jdk-noble
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates curl maven git && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://github.com/scip-code/scip-java/releases/download/v0.13.1/scip-java-v0.13.1 -o /usr/local/bin/scip-java && chmod +x /usr/local/bin/scip-java
ENV GRAPHIT_SCIP_FAMILY=java
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
