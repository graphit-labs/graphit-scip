FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM ruby:3.4-trixie
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git build-essential && rm -rf /var/lib/apt/lists/*
RUN gem install scip-ruby -v 0.4.8 --no-document && gem install bundler --no-document
ENV GRAPHIT_SCIP_FAMILY=ruby
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
