FROM mcr.microsoft.com/dotnet/sdk:9.0
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git curl && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh && bash /tmp/dotnet-install.sh --channel 8.0 --runtime dotnet --install-dir /usr/share/dotnet && rm /tmp/dotnet-install.sh
RUN dotnet tool install scip-dotnet --version 0.2.14 --tool-path /opt/scip-dotnet
ENV PATH="/opt/scip-dotnet:${PATH}" GRAPHIT_SCIP_FAMILY=dotnet
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
