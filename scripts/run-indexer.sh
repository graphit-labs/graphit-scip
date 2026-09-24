#!/bin/sh
set -eu

cd /workspace
output=/output/index.scip
rm -f "$output"
case "${GRAPHIT_SCIP_FAMILY:?family is required}" in
  go)
    export GOPATH=/cache/go GOCACHE=/cache/go-build
    scip-go index --output "$output"
    ;;
  typescript)
    export npm_config_cache=/cache/npm COREPACK_HOME=/cache/corepack
    if [ ! -f tsconfig.json ]; then
      if [ ! -f /cache/graphit-tsconfig.json ]; then
        printf '{"compilerOptions":{"allowJs":true},"include":["/workspace/**/*"],"exclude":["/workspace/**/node_modules","/workspace/**/.git","/workspace/**/.graphit","/workspace/**/.build"]}\n' > /cache/graphit-tsconfig.json
      fi
      scip-typescript index --cwd /workspace --output "$output" /cache/graphit-tsconfig.json
    elif [ -f pnpm-lock.yaml ]; then
      scip-typescript index --pnpm-workspaces --output "$output"
    elif [ -f yarn.lock ]; then
      scip-typescript index --yarn-workspaces --output "$output"
    else
      scip-typescript index --output "$output"
    fi
    ;;
  python)
    export npm_config_cache=/cache/npm PIP_CACHE_DIR=/cache/pip
    if [ ! -x /cache/venv/bin/python3 ]; then python3 -m venv /cache/venv; fi
    export PATH="/cache/venv/bin:$PATH"
    if [ -f requirements.txt ]; then pip install -r requirements.txt; fi
    # Editable installs write metadata into the source tree. Dependencies from
    # requirements.txt still install into the cached virtual environment.
    project_version=$(python3 -c 'import pathlib, tomllib; p=pathlib.Path("pyproject.toml"); d=tomllib.loads(p.read_text()) if p.exists() else {}; print(d.get("project", {}).get("version") or d.get("tool", {}).get("poetry", {}).get("version") or "0.0.0")')
    scip-python index --cwd /workspace --output "$output" --project-name="${GRAPHIT_SCIP_PROJECT_NAME:-workspace}" --project-version="$project_version"
    ;;
  java)
    export MAVEN_OPTS='-Dmaven.repo.local=/cache/maven'
    export GRADLE_USER_HOME=/cache/gradle
    scip-java index --output="$output" --targetroot=/cache/java-target
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
      python3 /usr/local/bin/graphit-filter-clang-compdb "$compdb" /cache/graphit-compile-commands.json
      compdb=/cache/graphit-compile-commands.json
    fi
    scip-clang --compdb-path="$compdb" --index-output-path="$output"
    ;;
  dotnet)
    export DOTNET_CLI_HOME=/cache/dotnet NUGET_PACKAGES=/cache/nuget
    # scip-dotnet currently logs restore failures but may still exit zero with
    # a degraded index. Restore explicitly so Graphit can use syntax fallback.
    for project in /workspace/*.sln /workspace/*.slnx /workspace/*.csproj /workspace/*.vbproj; do
      [ -f "$project" ] || continue
      dotnet restore "$project" /p:EnableWindowsTargeting=true
    done
    scip-dotnet index --output "$output" --skip-dotnet-restore
    ;;
  rust)
    export CARGO_HOME=/cache/cargo CARGO_TARGET_DIR=/cache/target RUSTUP_HOME=/usr/local/rustup
    rust-analyzer scip . --output "$output"
    ;;
  ruby)
    export BUNDLE_PATH=/cache/bundle BUNDLE_APP_CONFIG=/cache/bundle-config BUNDLE_GEMFILE=/cache/scip-ruby.Gemfile
    printf 'source "https://rubygems.org"\n' > "$BUNDLE_GEMFILE"
    if [ -f Gemfile ]; then printf 'eval_gemfile "/workspace/Gemfile"\n' >> "$BUNDLE_GEMFILE"; fi
    printf 'gem "scip-ruby", "0.4.8"\n' >> "$BUNDLE_GEMFILE"
    bundle install
    gem_metadata=$(ruby -e 'f=Dir.glob("*.gemspec").first; s=Gem::Specification.load(f) if f; print "#{s.name}@#{s.version}" if s')
    if [ -z "$gem_metadata" ]; then gem_metadata="${GRAPHIT_SCIP_PROJECT_NAME:-workspace}@0.0.0"; fi
    if [ -f sorbet/config ]; then bundle exec scip-ruby --gem-metadata "$gem_metadata" --index-file "$output"; else bundle exec scip-ruby --gem-metadata "$gem_metadata" --index-file "$output" .; fi
    ;;
  dart)
    export PUB_CACHE=/cache/pub
    if [ ! -d "$PUB_CACHE/global_packages/scip_dart" ]; then
      dart pub global activate scip_dart 1.6.2
    fi
    dart pub global run scip_dart --output "$output" ./
    ;;
  php)
    export COMPOSER_HOME=/cache/composer COMPOSER_CACHE_DIR=/cache/composer-cache
    if [ ! -f composer.json ] || [ ! -f composer.lock ]; then
      echo 'PHP SCIP requires composer.json and composer.lock' >&2
      exit 2
    fi
    # PHP variables must reach the interpreter literally.
    # shellcheck disable=SC2016
    php -r 'require "/opt/scip-php/vendor/autoload.php"; $indexer = new \ScipPhp\Indexer("/workspace", "0.0.1", []); file_put_contents("/output/index.scip", $indexer->index()->serializeToString());'
    ;;
  *) echo "unknown SCIP family: $GRAPHIT_SCIP_FAMILY" >&2; exit 2 ;;
esac

if [ ! -s "$output" ]; then
  echo 'indexer did not produce index.scip' >&2
  exit 1
fi
