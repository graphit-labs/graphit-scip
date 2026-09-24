FROM ruby:3.4-trixie
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git build-essential && rm -rf /var/lib/apt/lists/*
RUN gem install scip-ruby -v 0.4.8 --no-document && gem install bundler --no-document
ENV GRAPHIT_SCIP_FAMILY=ruby
COPY scripts/entrypoint.sh /usr/local/bin/graphit-scip-index
COPY scripts/run-indexer.sh /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
