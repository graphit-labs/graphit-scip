# Graphit SCIP images

Graphit Code indexes code through SCIP by default. These specialized Docker
producers supply compiler indexes; missing or failed native results fall back
to the configured Tree-sitter or ANTLR parsers, which also emit SCIP. There is
no SCIP opt-in flag. Syntax profiles continue to supply Graphit's additional
relationships, documentation, metrics and embedding projections.

Each image reads a project through `/workspace` and writes the SCIP protobuf
directly to `/output/index.scip`. Graphit Code mounts the source read-only at
`/source` and gives the image a fresh ephemeral mount at `/overlay`. With
`GRAPHIT_SCIP_OVERLAY=1`, the shared entrypoint mounts OverlayFS at `/workspace`:
`/source` is the lower layer and `/overlay/upper` and `/overlay/work` are on the
Docker daemon's Linux filesystem. Build artifacts and temporary writes go to
the upper layer; OverlayFS copies only files actually modified, and the
original source stays unchanged. The temporary mount and
container are removed after each parse. `/output` and persistent `/cache`
remain in Graphit Code's global AST directory. The entrypoint requires either
the numeric pair `GRAPHIT_SCIP_UID` and `GRAPHIT_SCIP_GID`, or the explicit
`GRAPHIT_SCIP_OWNER=output-mount` mode with both numeric variables absent. That
mode reads the existing `/output` directory's Linux UID/GID inside the container
before creating directories, changing ownership or mounting the overlay. An
unknown mode, a mixed or incomplete selection, or invalid numeric IDs fails
before those effects. In `output-mount` mode, an absent or non-directory output
mount also fails before effects; the numeric-pair mode retains its later
directory creation. No UID/GID is inferred
from a Windows SID or replaced with zero. The entrypoint mounts the overlay
before dropping to the selected identity, all capabilities, and `no-new-privs`.
Docker must permit the
mount with `SYS_ADMIN` and `apparmor=unconfined`. An unsupported mount fails the
SCIP run so Graphit Code can use its syntax parser. The image also accepts a direct `/workspace` mount when overlay mode is omitted
for standalone use.
The merged tree retains POSIX permissions from source entries. A nested
directory that is mode `0555` for the indexer UID can still reject a write;
that indexer then falls back to syntax parsing and SCIP generation without
modifying the source.

For a standalone manual run on a native Linux Docker daemon, build the local
image and keep the output and cache outside the source project:

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

On Unix hosts, Graphit Code passes the actual output owner's numeric UID/GID.
On Windows it passes only `GRAPHIT_SCIP_OWNER=output-mount`; the Linux container
observes ownership of the bind mount instead of translating a Windows identity.
An observed UID/GID of zero is used only when it is the real mount owner; zero
also remains valid when explicitly supplied in the numeric pair. Both paths use
absolute host paths. The wrapper never
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
For C/C++, Graphit Code also supplies `GRAPHIT_SCIP_HOST_ROOT`; the wrapper
rewrites absolute host paths in `compile_commands.json` to `/workspace` and
stores the translated database in `/cache`. When Graphit Code supplies its
allowed-source manifest in `/cache`, the wrapper omits resolvable translation
units excluded by recursive ignore rules. Entries whose source path cannot be
resolved stay in the derived database for Graphit's final document filter.
The manifest and database are generated outside the source project. Linux,
Windows and macOS path formats are handled in the translation; Docker Desktop
execution on Windows/macOS still needs native smoke validation.
The clang image uses a Rust binary for this translation, with bounded input and
output and no Python filter dependency. See the [compilation database filter
reference](docs/compdb-filter.md) for invocation, limits, selection, errors and
focused validation.

The Rust `scip-run` dispatcher implements all ten indexer sequences, including
dependency caches, build-system priorities and output validation. The images
copy the native dispatcher and `scip-entrypoint`; upstream indexers and language
runtimes remain vendor tools. See the [dispatcher reference](docs/scip-dispatch.md)
for configuration, errors and focused image smoke tests.

The .NET image builds the [corrected external producer](docs/scip-dotnet-vendor.md)
from pinned source and installs its local net10 package, `0.2.14-graphit.2`.
Its smoke test exercises original and modern C#/Visual Basic projects using the
real dispatcher and official SCIP CLI.

| Image suffix | Languages | Indexer | Project prerequisites |
| --- | --- | --- | --- |
| `go` | Go | scip-go 0.2.7 | `go.mod` or configured Go packages driver |
| `typescript` | TypeScript, JavaScript | scip-typescript 0.4.0 | package manifest; `tsconfig.json` recommended |
| `python` | Python | scip-python 0.6.6 | Python 3.10+; requirements/pyproject installed when present |
| `java` | Java, Kotlin | scip-java 0.13.1 | supported Gradle/Maven build; JVM compatibility |
| `clang` | C, C++ | scip-clang 0.4.0 | `compile_commands.json` or CMake project |
| `dotnet` | C#, Visual Basic | scip-dotnet 0.2.14-graphit.2 | solution/project and restoreable dependencies |
| `rust` | Rust | rust-analyzer | Cargo project |
| `ruby` | Ruby | scip-ruby 0.4.8 | Gemfile/sorbet config optional, Sorbet improves precision |
| `dart` | Dart | scip_dart 1.6.2 | pubspec.yaml and package configuration from `dart pub get` |
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

On Windows, use host absolute paths and replace both UID/GID options in this
manual command with `-e GRAPHIT_SCIP_OWNER=output-mount`; `/output` must already
be mounted as a directory. Do not combine that option with either numeric
identity variable, even an empty one. On macOS, keep the actual Unix output
owner's numeric pair. For C/C++ compilation databases created on the host, also pass
`GRAPHIT_SCIP_HOST_ROOT=$project_path` to translate paths into `/workspace`.
Overlay mode has been verified with Maven on Linux. Native Docker Desktop
checks on Windows and macOS remain pending.

The pinned scip-java release indexes Java and Kotlin; its Scala support was
removed upstream, so Scala continues through Graphit Code's syntax grammar.

An indexer failure or a missing SCIP document lets Graphit Code use its
syntax parsers to generate SCIP for that file. The images contain the indexer and language
toolchain; project dependencies may need network access on the first run. The
`clang` and `ruby` binaries currently target Linux amd64, so the release
workflow builds that platform for those two families. The other images publish
Linux amd64 and arm64 variants. Docker Desktop can run Linux amd64 containers
on ARM hosts through emulation; indexing is slower and may hit emulation limits.
Full support for every project build system is
limited by its upstream indexer and project configuration.

The release workflow uses the Rust `release-metadata` executable to validate a
`vMAJOR.MINOR.PATCH` Git tag and discover regular, non-symlink
`images/*/Dockerfile` files. It generates a sorted family/platform matrix and
GHCR tags `MAJOR.MINOR.PATCH`, `MAJOR.MINOR`, `MAJOR`, `v1`, and `latest`.
The literal `v1` alias is the image tag Graphit Code currently selects by
default, independent of the release's semantic version. For example, `v0.2.1`
produces tags `0.2.1`, `0.2`, `0`, `v1`, and `latest`. Invalid versions or image
metadata stop the producer before Docker publication steps run. See the
[release metadata reference](docs/release-images.md) for local validation,
limits, workflow consumers and focused checks.

The .NET source build uses the pinned SDK and Roslyn sources, including the
13 localization satellites. Its four original/modern C# and Visual Basic
projects passed cold/warm indexing and official SCIP print/lint/stats. The
external producer, package attribution and supported SDK behavior are described
in the [vendor reference](docs/scip-dotnet-vendor.md).

## Development checks

Prepare the pinned toolchain and locked Cargo sources once, then run the
standard suite offline.

```sh
cargo fetch --locked
cargo run --locked --offline --target host-tuple --bin xtask -- check
```

The check runs formatting, Clippy, and Rust tests. Tests use private temporary
directories, the workspace binaries, and synthetic indexer processes. Rust runs
independent cases in parallel on the available CPUs. Set `RUST_TEST_THREADS` to
override test concurrency for a local run. `cargo test --locked --offline` runs
only the tests.

The pre-commit hook checks staged whitespace, workflows and formatting. Pre-push
runs the offline Rust check. GitHub runs the standard suite on Linux, Windows
and macOS, with workflow lint as a separate job.

Use the [image qualification workflow](.github/workflows/qualification.yml) or
`cargo run --locked --offline --bin xtask -- docker-smoke` to check the real clang
image. Use `images-smoke FAMILY` for another affected family and
`cargo audit --file Cargo.lock` for a separate dependency audit. Docker and the
language tools used by those qualifications are unnecessary for the standard
suite. Tagged releases require both the standard check and clang image
qualification before publishing.
