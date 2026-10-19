FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM node:24-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates curl git python3 python3-pip python3-venv && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://registry.npmjs.org/@sourcegraph/scip-python/-/scip-python-0.6.6.tgz -o /tmp/scip-python.tgz \
    && echo 'aa828bd51820834a399dec0015b08500a952d008d6c46e4c03e982dbc06d8313afd0384ee3374bf2eef5e75171129a43a57315be6efec974a35e8b68547f5c31  /tmp/scip-python.tgz' | sha512sum -c - \
    && npm install -g /tmp/scip-python.tgz \
    && rm /tmp/scip-python.tgz
ENV GRAPHIT_SCIP_FAMILY=python
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
