# Graphit SCIP images

Each image runs a language indexer against a project bind-mounted at
`/workspace` and writes a SCIP protobuf to `/output/index.scip`. The image
entrypoint requires `GRAPHIT_SCIP_UID` and `GRAPHIT_SCIP_GID`; it drops to that
identity before indexing. Mount a persistent, project-specific `/cache` for
package downloads and compiler/indexer caches. Graphit Code keeps a named
container per project and language family and restarts it on later parses;
it replaces the container when its image or mount contract changes.

For a standalone manual run on a native Linux Docker daemon:

```sh
docker run --rm \
  --mount "type=bind,src=$PWD,dst=/workspace" \
  --mount "type=bind,src=$PWD/.scip-output,dst=/output" \
  --mount "type=bind,src=$PWD/.scip-cache,dst=/cache" \
  -e GRAPHIT_SCIP_UID="$(id -u)" -e GRAPHIT_SCIP_GID="$(id -g)" \
  ghcr.io/graphit-labs/graphit-scip-go:0.2.1
```

Create the output and cache directories before running. On Windows and macOS,
Graphit Code passes UID/GID `0` for Docker Desktop bind-mount mapping; it also
uses absolute host paths. The wrapper preserves any pre-existing project
`index.scip`, runs the indexer, copies the result to `/output/index.scip`, and
restores the original project file. Graphit Code accepts only files selected by its own
recursive `.gitignore`/`.astignore` checker; an indexer may emit more files.
For C/C++, Graphit Code also supplies `GRAPHIT_SCIP_HOST_ROOT`; the wrapper
rewrites absolute host paths in `compile_commands.json` to `/workspace` and
stores the translated database in `/cache`. This lets compilation databases
generated on Linux, Windows, or macOS be read inside the Linux container.

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
| `php` | PHP | davidrjenni/scip-php commit `71a5b11` | composer.json, lock and installable dependencies |

To smoke test any family locally, prepare a small project satisfying its row
above, then run these commands from the `graphit-scip` repository. Set
`family` and `project_path` for that project; repeat for each Dockerfile family:

```sh
family=go
project_path=/absolute/path/to/project
smoke_root="/tmp/graphit-scip-$family"
docker build -t "graphit-scip-$family:dev" -f "images/$family/Dockerfile" .
mkdir -p "$smoke_root/output" "$smoke_root/cache"
docker run --rm \
  --mount "type=bind,src=$project_path,dst=/workspace" \
  --mount "type=bind,src=$smoke_root/output,dst=/output" \
  --mount "type=bind,src=$smoke_root/cache,dst=/cache" \
  -e GRAPHIT_SCIP_UID="$(id -u)" -e GRAPHIT_SCIP_GID="$(id -g)" \
  "graphit-scip-$family:dev"
test -s "$smoke_root/output/index.scip"
```

On Windows/macOS, use host absolute paths and UID/GID `0` in this manual
command. For C/C++ compilation databases created on the host, also pass
`GRAPHIT_SCIP_HOST_ROOT=$project_path` to translate paths into `/workspace`.

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
`MAJOR.MINOR`, `MAJOR`, and `latest` for each. For example, after creating
`v0.2.1`, run `git push origin refs/tags/v0.2.1`; the resulting image tags
are `0.2.1`, `0.2`, `0`, and `latest`. A matching tag with an invalid
semantic version fails before publication.
