---
title: SCIP indexer dispatch
type: reference
references:
  - target: {type: task, id: tsk-d36a, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: implements
  - target:
      type: task
      id: tsk-a7ba
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
  - target:
      type: task
      id: tsk-402c
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
  - target: {type: task, id: tsk-cab0, scope: project, scope_id: 01KSH1CRFFG8Z74B5ZS78WW808}
    relation: derived_from
---

# SCIP indexer dispatch

`scip-run` takes no arguments. `GRAPHIT_SCIP_FAMILY` selects exactly one of
`go`, `typescript`, `python`, `java`, `clang`, `dotnet`, `rust`, `ruby`, `dart`,
or `php`. The CLI uses `/workspace`, persistent `/cache`, and
`/output/index.scip`. These directories must exist; the shared entrypoint
prepares cache directories and drops privileges before invoking it. The
library `dispatch::run(&Settings, &Control)` accepts explicit absolute paths,
PATH, PHP autoload path, host root and project name per invocation. It never
changes the process-global current directory or environment.

All ten Dockerfiles build with `rust:1.98.0-slim-bookworm` and copy the Rust
executables into their vendor runtime. Development lint components stay in
the local toolchain configuration.
The image matrix records real upstream Linux amd64 validation. The .NET vendor
uses SDK 10.0.401 and Roslyn 5.9 with package `0.2.14-graphit.2`; its C#/VB corpus
passes official SCIP conformance. The shared Rust `scip-entrypoint` resolves and validates ownership, prepares an empty
OverlayFS upper/work directory, sets cache environment and ownership, then
uses the system `mount` and `setpriv` tools. Unix `exec` keeps the dispatcher
as container PID 1 and preserves signals and exit codes. See [the vendor evidence](scip-dotnet-vendor.md).

## Entrypoint ownership

Select exactly one ownership source before the entrypoint performs any directory
creation, chown or OverlayFS mount:

| Environment | Identity used |
| --- | --- |
| `GRAPHIT_SCIP_OWNER` absent, both `GRAPHIT_SCIP_UID` and `GRAPHIT_SCIP_GID` present | The explicit numeric pair, including zero when genuinely supplied. Both values must contain only ASCII digits and fit `0..4294967295` (exclusive upper bound). |
| `GRAPHIT_SCIP_OWNER=output-mount`, both numeric variables absent | UID/GID observed by Linux `fs::metadata("/output")` inside the container. `/output` must already exist as a directory; both observed IDs must fit the same range. |

Windows callers use the explicit `output-mount` mode because their host identity
does not supply a Unix UID/GID pair. Unix callers retain the actual output
owner's numeric pair. The entrypoint does not translate a SID or invent zero;
zero observed on the Linux mount remains a genuine identity. Metadata is read
before effects, not after a chown has replaced the owner being observed.

An empty, unknown or non-Unicode owner mode is invalid. Combining `output-mount`
with either numeric variable, even an empty value, is ambiguous and fails before
reading metadata. A missing/invalid numeric pair also fails before effects.
In `output-mount` mode, a missing/non-directory output mount fails before
effects; numeric-pair mode retains its later directory creation. There is no
fallback between modes.
After resolution, both modes follow the same existing mount, cache, `setpriv`
(`--no-new-privs`, no bounding capabilities, selected UID/GID, cleared groups)
and `exec` path. The source mount, output path and dispatcher arguments do not
change. Native Docker Desktop execution remains a separate validation.

## Journeys

| Family | Sequence and selection | Child environment under `/cache` |
| --- | --- | --- |
| Go | `scip-go index --output OUTPUT` | `GOPATH=go`, `GOCACHE=go-build` |
| TypeScript | With tsconfig: pnpm lock takes priority over yarn lock, selecting `--pnpm-workspaces` or `--yarn-workspaces`; otherwise plain index. Without tsconfig, use the exact existing `graphit-tsconfig.json` supplied by Code, or generate allowJs and workspace includes excluding node_modules/.git/.graphit/.build. Pass `--cwd WORKSPACE --output OUTPUT CONFIG`. | `npm_config_cache=npm`, `COREPACK_HOME=corepack` |
| Python | Create venv only when its python3 is not executable, prepend its bin to PATH, install requirements when present. Rust TOML selects nonempty project.version, then tool.poetry.version, then 0.0.0. `scip-python index --cwd WORKSPACE --output OUTPUT --project-name=NAME --project-version=VERSION`. | `npm_config_cache=npm`, `PIP_CACHE_DIR=pip`, venv at `venv` |
| Java | `scip-java index --output=OUTPUT --targetroot=CACHE/java-target` | `MAVEN_OPTS=-Dmaven.repo.local=CACHE/maven`, `GRADLE_USER_HOME=gradle` |
| Clang | Root compile_commands.json before build/compile_commands.json before CMake configure into cmake-build. With nonempty host root, run the maintained Rust filter into graphit-compile-commands.json. `scip-clang --compdb-path=COMPDB --index-output-path=OUTPUT`. | `GRAPHIT_SCIP_HOST_ROOT` passed to filter; allowed-source manifest remains at its existing cache location |
| .NET | Discover regular files recursively in .sln, .slnx, .csproj, .vbproj priority, sorted by full path within each group; omit links, hidden names and generated bin/obj/node_modules/target directories. Use SDK `dotnet sln SOLUTION list` to exclude projects already covered by solutions. Restore selected inputs with `/p:EnableWindowsTargeting=true`, abort on any failure, then one `scip-dotnet index --output OUTPUT --skip-dotnet-restore INPUTS...` from the workspace root. | `DOTNET_CLI_HOME=dotnet`, `NUGET_PACKAGES=nuget` |
| Rust | `rust-analyzer scip . --output OUTPUT` | `CARGO_HOME=cargo`, `CARGO_TARGET_DIR=target`, `RUSTUP_HOME=/usr/local/rustup` |
| Ruby | Generate cached Gemfile with scip-ruby 0.4.8; evaluate source Gemfile through constant `ENV.fetch("GRAPHIT_SCIP_SOURCE_GEMFILE")`. Bundle install, read first sorted root gemspec through RubyGems API, fall back to NAME@0.0.0 when absent/nil. `bundle exec scip-ruby --gem-metadata META --index-file OUTPUT` and trailing dot only without sorbet/config. | `BUNDLE_PATH=bundle`, `BUNDLE_APP_CONFIG=bundle-config`, `BUNDLE_GEMFILE=scip-ruby.Gemfile`, source Gemfile path in separate env |
| Dart | Activate scip_dart 1.6.2 only if global package cache directory is absent; `dart pub global run scip_dart --output OUTPUT ./`. | `PUB_CACHE=pub` |
| PHP | Require composer.json and composer.lock. Invoke pinned vendor Indexer with workspace, version 0.0.1 and empty options; serialize directly to OUTPUT. | `COMPOSER_HOME=composer`, `COMPOSER_CACHE_DIR=composer-cache` |

The Rust image installs both `rust-analyzer` and `rust-src` for its toolchain.
Otherwise rust-analyzer attempts to download `rust-src` during sysroot discovery,
making an indexing request depend on network access. Installing the component
when building the image supplies the standard-library sources directly.

`GRAPHIT_SCIP_PROJECT_NAME` defaults to `workspace` when absent/empty. CLI
project names must be UTF-8; invalid bytes fail configuration validation.
Python metadata with wrong types or malformed TOML fails validation; arbitrary
Python truthiness coercion was removed. Dependency caches persist across runs.
Preflight prepares TypeScript JSON only when it must be generated, and Java
Maven options only for that family. Their required UTF-8 conversions complete
before any output removal or child/cache writes. Native opaque paths remain
accepted by branches that pass OS paths directly through argv or environment.
Python uses `venv/bin/python3` on Unix and `venv/Scripts/python.exe` on Windows;
the corresponding executable directory is prepended to its child PATH. It
serializes that directory plus inherited PATH using the standard OS
split/join APIs and admits the complete serialized value to the 128 KiB cap
before output removal or venv creation. Invalid platform quoting/separators
fail as configuration errors. Rejected configurations preserve previous output
and start no subprocess.
Stale output is removed after preflight; success requires a regular nonempty
index. A failed upstream process can leave partial output, which is never
accepted. Graphit Code owns syntax fallback and container/overlay removal.

All subprocesses use structured arguments, inherited stdin/stdout/stderr and
child-only environment updates. Ruby metadata and SDK solution-list stdout
use bounded capture. PHP and Ruby use constant minimal vendor API calls, with paths passed
through argv or environment. This accommodates vendor CLIs lacking the needed
API without adding maintained interpreter files or shell command parsers.
Upstream indexers, RubyGems and PHP runtime APIs remain vendor dependencies.

## Errors and resource boundaries

| Exit | Meaning |
| --- | --- |
| 0 | Upstream sequence succeeded and nonempty regular output exists |
| 1 | I/O, process cleanup, invalid/missing output |
| 2 | Invalid configuration, required input, metadata type or admission cap |
| 126 / 127 | Permission denied / missing executable |
| 130 / 124 | Cooperative cancellation / whole-sequence deadline |
| Upstream numeric status | Vendor failure; Unix signals map to 128 + signal |

Own diagnostics do not echo rejected values, commands, paths or credentials.
Process failures identify the operation, OS error kind and numeric OS error.
Vendors
retain their own diagnostics. PATH belongs to each Settings instance; Python
extends only its children's PATH. Proxies and vendor credentials remain
inherited as before.

Admission caps: metadata 32 KiB, each path/PATH 128 KiB, TOML input 16 MiB,
captured metadata/solution-list stdout 16 KiB, matched input discovery 4096 files and generated
configuration 64 KiB. Checks occur before retaining additional own data.
TOML uses its vendor recursion guard of 80; the unbounded feature is disabled.
The TOML parser can allocate transient parsing data within admitted input;
version fields are checked before cloning into retained metadata. These are
logical admission limits, not hard RSS/CPU/disk/network quotas on vendors.
Native OS invocation limits also apply. In particular, Windows limits a complete
command line to 32,767 UTF-16 code units, including executable, quoting, flags and
paths. A field below the metadata cap can therefore still exceed that total;
the dispatcher reports the native size failure as exit 2, rather than ignoring
the value or attempting to run a different command. Use shorter metadata or
paths when this limit is reached. The container images execute on Linux.

`GRAPHIT_SCIP_TIMEOUT_MS`, when present, must be a positive u64 milliseconds
value. The deadline starts once, covering dependency installs, build setup,
metadata and indexing. Without it, runs have no duration limit. SIGINT,
SIGTERM and SIGHUP set the shared cancellation flag. Sequential execution
permits at most one active child group; polling checks every 1 ms during the
first second of each process and every 20 ms thereafter. This permits at most
about 1000 polling wakeups in that first second, then
about 50 per second, trading a small startup/control cost for prompt short
process completion without busy spinning.
Filesystem and TOML work check before/after synchronous operations. A blocked NFS/kernel
D-state syscall or synchronous parser is not forcibly interrupted; this is a
cooperative deadline, with no hard wall-time guarantee.

`process-wrap` owns a Unix process group or Windows Job Object. A Rust guard
terminates the group and reaps the direct child on success, failure and drop,
including a leader that exits while a descendant holds captured stdout open.
Only Unix ESRCH (group already gone) is ignored; cleanup permission errors
fail. Capture drain polls for at most one second and never performs an
unconditional reader join. Descendants are terminated; only the direct child
is owned/reaped by this process. Escaped sessions, owner SIGKILL and hostile
filesystem atomicity remain outside this boundary.

## Focused validation

`cargo test --locked --bin scip-entrypoint` exercises the existing numeric
validation and, on Linux, ownership selection against real filesystem metadata,
mutual exclusion, invalid modes and missing/non-directory output. These tests
do not require Docker or claim that a bind mount, OverlayFS or Docker Desktop
has been exercised. The entrypoint uses only `std`, so its unit suite can also
be compiled directly with `rustc --edition=2024 --test src/bin/scip-entrypoint.rs`
and run from a caller-selected output path.

After `cargo fetch --locked`, run the offline standard check with
`cargo run --locked --offline --target host-tuple --bin xtask -- check`.
The three native CI jobs cache locked Cargo sources separately from compiled
Rust libraries, build-script outputs and the actual tool executables. Compiled
keys retain OS/architecture, Rust host/toolchain, runner image, linker/SDK,
build options, manifests, lockfile and workflow; only the source revision suffix
is relaxed during a compatible restore. Incremental state and test executables
are excluded. SCIP has no native SDK or vcpkg producer to cache in this workflow.
Cargo may rebuild executable wrappers after restore; a cache hit does not
guarantee that every target is fresh.
Windows selects the x64 Visual Studio 2022 environment before probing `cl /Bv`;
the MSVC toolset, Windows SDK and UCRT versions must be nonempty to form a key.

CI fetches locked inputs even on a cache hit, prepares Clippy and test builds
offline, and explicitly saves compiled outputs before executing the unchanged
`xtask check` gate. Formatting, Clippy and all ordinary tests still run on every
job; cache misses use the same locked/offline path. The Docker image smoke job
stays separate, and release images retain their existing Buildx caches. Report
cache transfer, compilation and test execution separately when measuring;
local warm preparation does not qualify hosted macOS/Windows performance.

For the dispatch suite alone, use `cargo test --locked --offline --test dispatch`.
Its independent cases exercise the real dispatcher and synthetic processes,
including command order, cache reuse, argument and environment isolation,
errors, deadlines, cancellation and descendant-pipe cleanup.
The subprocess fixture is compiled with the pinned Rust toolchain and cached
by source, toolchain, OS and architecture beneath Cargo's test temporary root.
Each dispatch gets its own executable inodes, workspace, cache and output.
Cancellation waits for the fixture's readiness marker. Descendant cleanup is
checked through process state on Linux and macOS, accepting a reaped process
or a terminated zombie. The macOS check uses the operating system's `/bin/ps`.

The installed Linux Ruby and PHP adapter test is an explicit qualification:
`cargo test --locked --offline --test dispatch real_ruby_gemspec_and_php_vendor_api_receive_literal_paths -- --ignored --exact`.
The PHP fixture models the vendor API shape. Qualify the actual PHP image with
`cargo run --locked --offline --bin xtask -- images-smoke php`.

The separate `docker-smoke` command builds the clang image and exercises real
indexing, OverlayFS, ownership, errors, deadlines, PID 1 and SIGTERM. Run image
qualification for the affected family. Run dependency audit separately with
`cargo audit --file Cargo.lock`.
The nine upstream families have retained real Linux amd64 qualifications.
Native macOS/Windows Job behavior, Docker Desktop and arm64 require their
actual validations. Documentation conversion,
classification and freshness bugs in the current Knowledge producer remain
tracked under tsk-69f2; authoritative source docs must be checked separately
from generated Wiki lint until those defects are resolved.

## Image smoke tests

Before accepting an upstream index, the shared dispatcher completes the
[document position-encoding contract](scip-position-encoding.md) for producers
whose pinned source or verified artifact establishes the units. This preserves explicit encodings
and never rebases coordinates. Ruby's implicit UTF8 units are qualified against
the checksum-verified 0.4.8 artifact; unspecified Rust units remain rejected.
This does not turn fixture coverage into image qualification.

`cargo run --locked --bin xtask -- images-smoke` builds and exercises the nine
additional indexer families sequentially. Select one family for focused work,
for example `cargo run --locked --bin xtask -- images-smoke go`. A family may
also take an existing `sha256:<64 lowercase hex digits>` image ID, avoiding a build.

The runner uses real source projects, cold/warm indexing, a cache marker and
source preservation checks. The official `scip print --json` and `scip lint`
commands check each nonempty index. Invalid family and deadline cases exercise
the dispatcher's exit and stale-output behavior. Java typed ranges are covered
by a small decoded fixture and a unit test.

The .NET smoke exercises original and modern C#/VB projects. Dart and PHP
prepare their inputs with `dart pub get` and Composer; .NET uses `dotnet restore`.
NuGet restoration may require network access and has a two-minute timeout. The dispatcher does not add these
fixture preparation steps to user projects.

Sources, caches and decoded output live in disposable temporary directories.
Containers use `docker run --rm` without CPU throttling or aggregate process
memory/PID quotas. Source remains read-only during indexing; preparation writes
only the disposable fixture. Cache and output use separate mounts, and the
entrypoint keeps the existing UID/GID and OverlayFS mount boundary.

The fresh overlay tmpfs retains `nosuid,nodev,size=1g`: this bounds the actual
temporary files in its upper/work layers so generated build files cannot grow
without limit in a memory-backed filesystem. It reserves no process resources.
Restore uses the SDK’s normal concurrency. Restore and indexing deadlines
still check functional termination and cleanup.
No execution receipts, resource accounting or historical qualification reports
are required. Build output is streamed to the terminal.

Integration requires Linux amd64 Docker with OverlayFS mount permission and the
official `scip` CLI on PATH. Native arm64, macOS/Windows and Docker Desktop remain
separate integration validations.
