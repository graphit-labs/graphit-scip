FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM php:8.3-cli-bookworm
COPY --from=composer:2 /usr/bin/composer /usr/local/bin/composer
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git unzip libzip-dev && docker-php-ext-install zip && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /opt/scip-php && COMPOSER_HOME=/opt/scip-php composer global require 'davidrjenni/scip-php:dev-main#71a5b117ec4c5dd2af302e363410e604e5df309e' --no-interaction --prefer-dist
ENV GRAPHIT_SCIP_FAMILY=php
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
