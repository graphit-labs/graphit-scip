#!/bin/sh
set -eu

cd /workspace
original_index=
if [ -e index.scip ]; then
  original_index=$(mktemp /cache/scip-original.XXXXXX)
  cp -p index.scip "$original_index"
fi
restore_index() {
  if [ -n "$original_index" ]; then
    cp -p "$original_index" index.scip
    rm -f "$original_index"
  else
    rm -f index.scip
  fi
}
trap restore_index EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
rm -f index.scip
case "${GRAPHIT_SCIP_FAMILY:?family is required}" in
  go)
    export GOPATH=/cache/go GOCACHE=/cache/go-build
    scip-go
    ;;
  typescript)
    export npm_config_cache=/cache/npm COREPACK_HOME=/cache/corepack
    if [ -f pnpm-lock.yaml ]; then
      corepack pnpm install --prefer-offline
      scip-typescript index --pnpm-workspaces
    elif [ -f yarn.lock ]; then
      corepack yarn install
      scip-typescript index --yarn-workspaces
    else
      if [ -f package-lock.json ]; then
        npm install --prefer-offline
      elif [ -f package.json ]; then
        npm install --prefer-offline --no-package-lock
      fi
      if [ -f tsconfig.json ]; then scip-typescript index; else scip-typescript index --infer-tsconfig; fi
    fi
    ;;
  python)
    export npm_config_cache=/cache/npm PIP_CACHE_DIR=/cache/pip
    if [ ! -x /cache/venv/bin/python3 ]; then python3 -m venv /cache/venv; fi
    export PATH="/cache/venv/bin:$PATH"
    if [ -f requirements.txt ]; then pip install -r requirements.txt; fi
    if [ -f pyproject.toml ] || [ -f setup.py ]; then pip install -e .; fi
    project_version=$(python3 -c 'import pathlib, tomllib; p=pathlib.Path("pyproject.toml"); d=tomllib.loads(p.read_text()) if p.exists() else {}; print(d.get("project", {}).get("version") or d.get("tool", {}).get("poetry", {}).get("version") or "0.0.0")')
    scip-python index . --project-name="${GRAPHIT_SCIP_PROJECT_NAME:-workspace}" --project-version="$project_version"
    ;;
  java)
    export MAVEN_OPTS='-Dmaven.repo.local=/cache/maven'
    export GRADLE_USER_HOME=/cache/gradle
    scip-java index
    ;;
  clang)
    if [ -f compile_commands.json ]; then
      compdb=compile_commands.json
    elif [ -f build/compile_commands.json ]; then
      compdb=build/compile_commands.json
    elif [ -f CMakeLists.txt ]; then
      cmake -S . -B /cache/cmake-build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
      compdb=/cache/cmake-build/compile_commands.json
    else
      echo 'C/C++ indexing requires compile_commands.json or CMakeLists.txt' >&2
      exit 2
    fi
    if [ -n "${GRAPHIT_SCIP_HOST_ROOT:-}" ]; then
      python3 - "$compdb" /cache/graphit-compile-commands.json <<'PY'
import json
import os
import re
import sys

source, output = sys.argv[1:]
host_root = os.environ['GRAPHIT_SCIP_HOST_ROOT']
host_roots = [host_root]
windows_root = bool(re.match(r'^[A-Za-z]:[\\/]', host_root))
if windows_root:
    host_roots = [host_root.replace('\\', '/'), host_root.replace('/', '\\')]
host_roots = sorted(set(host_roots), key=len, reverse=True)
def container_path(value):
    for prefix in host_roots:
        if re.match(r'^[A-Za-z]:[\\/]', prefix):
            value = re.sub(re.escape(prefix), '/workspace', value, flags=re.IGNORECASE)
        else:
            value = value.replace(prefix, '/workspace')
    if windows_root and '/workspace' in value:
        value = value.replace('\\', '/')
    return value
with open(source, encoding='utf-8') as stream:
    commands = json.load(stream)
for command in commands:
    for key in ('directory', 'file', 'command'):
        if isinstance(command.get(key), str):
            command[key] = container_path(command[key])
    if isinstance(command.get('arguments'), list):
        command['arguments'] = [container_path(arg) for arg in command['arguments']]
with open(output, 'w', encoding='utf-8') as stream:
    json.dump(commands, stream)
PY
      compdb=/cache/graphit-compile-commands.json
    fi
    scip-clang --compdb-path="$compdb"
    ;;
  dotnet)
    export DOTNET_CLI_HOME=/cache/dotnet NUGET_PACKAGES=/cache/nuget
    scip-dotnet index
    ;;
  rust)
    export CARGO_HOME=/cache/cargo CARGO_TARGET_DIR=/cache/target RUSTUP_HOME=/usr/local/rustup
    rust-analyzer scip .
    ;;
  ruby)
    export BUNDLE_PATH=/cache/bundle BUNDLE_APP_CONFIG=/cache/bundle-config BUNDLE_GEMFILE=/cache/scip-ruby.Gemfile
    printf 'source "https://rubygems.org"\n' > "$BUNDLE_GEMFILE"
    if [ -f Gemfile ]; then printf 'eval_gemfile "/workspace/Gemfile"\n' >> "$BUNDLE_GEMFILE"; fi
    printf 'gem "scip-ruby", "0.4.8"\n' >> "$BUNDLE_GEMFILE"
    bundle install
    gem_metadata=$(ruby -e 'f=Dir.glob("*.gemspec").first; s=Gem::Specification.load(f) if f; print "#{s.name}@#{s.version}" if s')
    if [ -z "$gem_metadata" ]; then gem_metadata="${GRAPHIT_SCIP_PROJECT_NAME:-workspace}@0.0.0"; fi
    if [ -f sorbet/config ]; then bundle exec scip-ruby --gem-metadata "$gem_metadata"; else bundle exec scip-ruby --gem-metadata "$gem_metadata" .; fi
    ;;
  dart)
    export PUB_CACHE=/cache/pub
    if [ ! -d "$PUB_CACHE/global_packages/scip_dart" ]; then
      dart pub global activate scip_dart 1.6.2
    fi
    dart pub get
    dart pub global run scip_dart ./
    ;;
  php)
    export COMPOSER_HOME=/cache/composer COMPOSER_CACHE_DIR=/cache/composer-cache
    if [ ! -f composer.json ] || [ ! -f composer.lock ]; then
      echo 'PHP SCIP requires composer.json and composer.lock' >&2
      exit 2
    fi
    composer install --no-interaction --prefer-dist
    /opt/scip-php/vendor/bin/scip-php
    ;;
  *) echo "unknown SCIP family: $GRAPHIT_SCIP_FAMILY" >&2; exit 2 ;;
esac

if [ ! -s index.scip ]; then
  echo 'indexer did not produce index.scip' >&2
  exit 1
fi
cp index.scip /output/index.scip
