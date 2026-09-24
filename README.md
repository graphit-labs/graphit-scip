# Graphit SCIP images

Each image reads a project through `/workspace` and writes the SCIP protobuf
directly to `/output/index.scip`. Graphit Code mounts the source read-only at
`/source` and gives the image a fresh Docker volume at `/overlay`. With
`GRAPHIT_SCIP_OVERLAY=1`, the shared entrypoint mounts OverlayFS at `/workspace`:
`/source` is the lower layer and `/overlay/upper` and `/overlay/work` are on the
Docker daemon's Linux filesystem. Build artifacts and temporary writes go to
the upper layer; OverlayFS copies only files actually modified, and the
original source stays unchanged. The volume and
container are removed after each parse. `/output` and persistent `/cache`
remain in Graphit Code's global AST directory. The entrypoint requires
`GRAPHIT_SCIP_UID` and `GRAPHIT_SCIP_GID`; it mounts the overlay before dropping
to that identity, all capabilities, and `no-new-privs`. Docker must permit the
mount with `SYS_ADMIN` and `apparmor=unconfined`. An unsupported mount fails the
SCIP run so Graphit Code can use its syntax parser. Graphit Code also binds
the source read-only at `/workspace` for older images that do not recognize
overlay mode. The image accepts that direct mount when overlay mode is omitted
for standalone use.
The merged tree retains POSIX permissions from source entries. A nested
directory that is mode `0555` for the indexer UID can still reject a write;
that indexer then falls back to syntax parsing without modifying the source.

For a standalone manual run on a native Linux Docker daemon, build the local
image and keep the output and cache outside the source project. The published
`v1` image predates overlay mode:

```sh
artifact_root="${GRAPHIT_GLOBAL_DIR:-$HOME/.graphit}/ast/scip-smoke"
project_path=/absolute/path/to/project
mkdir -p "$artifact_root/output" "$artifact_root/cache"
docker build -t graphit-scip-go:dev -f images/go/Dockerfile .
docker run --rm --cap-add=SYS_ADMIN --security-opt=apparmor=unconfined \
  --mount "type=bind,src=$project_path,dst=/source,readonly" \
  --mount "type=volume,dst=/overlay,volume-nocopy" \
  --mount "type=bind,src=$artifact_root/output,dst=/output" \
  --mount "type=bind,src=$artifact_root/cache,dst=/cache" \
  -e GRAPHIT_SCIP_OVERLAY=1 \
  -e GRAPHIT_SCIP_UID="$(id -u)" -e GRAPHIT_SCIP_GID="$(id -g)" \
  graphit-scip-go:dev
```

On Windows and macOS, Graphit Code passes UID/GID `0` for Docker Desktop
bind-mount mapping; it also uses absolute host paths. The wrapper never
writes `index.scip` into the project. TypeScript without `tsconfig.json` uses
an inferred configuration in `/cache` with root files selected by Graphit's
recursive ignore checker. Home, temporary files and XDG caches
also live under `/cache`. Indexers can write beside source files in the merged
workspace; those changes disappear with the temporary volume. Graphit Code
accepts only files selected by its own
recursive `.gitignore`/`.astignore` checker, including nested rules and
negations. The image still receives the readable source tree, so an indexer may
read ignored dependencies. Other families and TypeScript projects with their
own configuration may include ignored documents in raw `index.scip`; Graphit
always filters documents again when importing them into its graph.
Graphit Code tracks the selected TypeScript/JavaScript roots, the project's
`tsconfig.json`, and discovered `tsconfig*.json`/`jsconfig*.json` files in its
global AST cache. Ignore/config changes and source
removals refresh that family; when no roots remain, it clears the old raw
`index.scip`. Scoped watcher edits also apply nested ignore rules before
SCIP or syntax parsing.
The [ignore prefilter audit](docs/ignore-prefilter-audit.md) records each
indexer's selection options, measured fixtures, and the remaining work.
For C/C++, Graphit Code also supplies `GRAPHIT_SCIP_HOST_ROOT`; the wrapper
rewrites absolute host paths in `compile_commands.json` to `/workspace` and
stores the translated database in `/cache`. When Graphit Code supplies its
allowed-source manifest in `/cache`, the wrapper omits resolvable translation
units excluded by recursive ignore rules. Entries whose source path cannot be
resolved stay in the derived database for Graphit's final document filter.
The manifest and database are generated outside the source project. Linux,
Windows and macOS path formats are handled in the translation; Docker Desktop
execution on Windows/macOS still needs native smoke validation.

| Image suffix | Languages | Indexer | Project prerequisites |
| --- | --- | --- | --- |
| `go` | Go | scip-go 0.2.7 | `go.mod` or configured Go packages driver |
| `typescript` | TypeScript, JavaScript | scip-typescript 0.4.0 | package manifest; `tsconfig.json` recommended |
| `python` | Python | scip-python 0.6.6 | Python 3.10+; requirements/pyproject installed when present |
| `java` | Java, Kotlin | scip-java 0.13.1 | supported Gradle/Maven build; JVM compatibility |
| `clang` | C, C++ | scip-clang 0.4.0 | `compile_commands.json` or CMake project |
| `dotnet` | C#, Visual Basic | scip-dotnet 0.2.14 | solution/project and restoreable dependencies |
| `rust` | Rust | rust-analyzer | Cargo project |
| `ruby` | Ruby | scip-ruby 0.4.8 | Gemfile/sorbet config optional, Sorbet improves precision |
| `dart` | Dart | scip_dart 1.6.2 | pubspec.yaml |
| `php` | PHP | davidrjenni/scip-php commit `71a5b11` | composer.json, lock and existing vendor directory |

To smoke test any family locally, prepare a small project satisfying its row
above, then run these commands from the `graphit-scip` repository. Set
`family` and `project_path` for that project; repeat for each Dockerfile family:

The manual command below uses the same fresh overlay mount as Graphit Code.

```sh
family=go
project_path=/absolute/path/to/project
smoke_root="/tmp/graphit-scip-$family"
docker build -t "graphit-scip-$family:dev" -f "images/$family/Dockerfile" .
mkdir -p "$smoke_root/output" "$smoke_root/cache"
docker run --rm --cap-add=SYS_ADMIN --security-opt=apparmor=unconfined \
  --mount "type=bind,src=$project_path,dst=/source,readonly" \
  --mount "type=volume,dst=/overlay,volume-nocopy" \
  --mount "type=bind,src=$smoke_root/output,dst=/output" \
  --mount "type=bind,src=$smoke_root/cache,dst=/cache" \
  -e GRAPHIT_SCIP_OVERLAY=1 \
  -e GRAPHIT_SCIP_UID="$(id -u)" -e GRAPHIT_SCIP_GID="$(id -g)" \
  "graphit-scip-$family:dev"
test -s "$smoke_root/output/index.scip"
```

On Windows/macOS, use host absolute paths and UID/GID `0` in this manual
command. For C/C++ compilation databases created on the host, also pass
`GRAPHIT_SCIP_HOST_ROOT=$project_path` to translate paths into `/workspace`.
Overlay mode has been verified with Maven on Linux. Native Docker Desktop
checks on Windows and macOS remain pending.

The pinned scip-java release indexes Java and Kotlin; its Scala support was
removed upstream, so Scala continues through Graphit Code's syntax grammar.

An indexer failure or a missing SCIP document lets Graphit Code use its
syntax parsers for that file. The images contain the indexer and language
toolchain; project dependencies may need network access on the first run. The
`clang` and `ruby` binaries currently target Linux amd64, so the release
workflow builds that platform for those two families. The other images publish
Linux amd64 and arm64 variants. Docker Desktop can run Linux amd64 containers
on ARM hosts through emulation; indexing is slower and may hit emulation limits.
Full support for every project build system is
limited by its upstream indexer and project configuration.

Pushing a `vMAJOR.MINOR.PATCH` Git tag discovers and builds every
`images/*/Dockerfile` and publishes GHCR tags `MAJOR.MINOR.PATCH`,
`MAJOR.MINOR`, `MAJOR`, and `latest` for each. Every release also
publishes the literal `v1` alias as a Graphit compatibility marker, independent
of its semantic version. For example, after creating
`v0.2.1`, run `git push origin refs/tags/v0.2.1`; the resulting image tags
are `0.2.1`, `0.2`, `0`, `v1`, and `latest`. A matching tag with an invalid
semantic version fails before publication.
