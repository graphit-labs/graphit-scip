# Roslyn MSBuild argument transport

The .NET producer distributes the MSBuild task and BuildHost built from
[`dotnet/roslyn` commit 35d9211](https://github.com/dotnet/roslyn/tree/35d9211b841e7613c1d2f8f5af6d628ace696c4c)
under the included MIT license. The upstream archive, patch and modified source
checksums are recorded in `upstream.json`. No complete Roslyn source checkout or
precompiled private diagnostic output is a maintained build dependency.

`ManagedToolTask` stores each complete original argument in literal
`GraphitRawCommandLineV1` metadata before assigning `ItemSpec`. The BuildHost
requires that metadata on each real item from the same MSBuild execution. This
preserves escaped argument text across the MSBuild item transport on Linux;
missing metadata fails before an index is produced.

The Docker recipe builds the official Task and BuildHost projects in order,
including the BuildHost's sole Contracts reference, with SDK 10.0.401 and:

```text
-c Release -m:1
-p:TargetFrameworks=net10.0
-p:NetCurrent=net10.0
-p:NetRoslynBuildHostNetCoreVersion=net10.0
-p:DotNetBuildSourceOnly=true
-p:DotNetBuildTests=false
-p:DotNetUseShippingVersions=true
```

The source SDK selection keeps the original pinned Arcade/MSBuild SDKs.

The package uses standard `PackAsTool` publish content. The 20 Task outputs live
under `GraphitRoslyn/Tasks`; the 20 BuildHost outputs live under `BuildHost-netcore`
next to the normal Workspaces.MSBuild assembly. All three Contracts build outputs
remain in build provenance. Its DLL/XML already appear identically in the Host
bundle, so they are not duplicated in a second installed directory. The Host's
own MSBuild.Locator dependency is 1.10.2; the producer's CLI dependency remains
1.7.8. Both dependencies are recorded separately.

The stock `BuildHost-net472` assets remain unchanged. The Linux SDK-style corpus
uses the netcore host; these tests do not qualify classic projects, Mono or native
Windows/macOS. A local build's actual assembly identities and hashes are recorded
as local outputs, without claiming identity with published NuGet binaries.

The workspace selects the corrected Task and targets with `RoslynTargetsPath`,
`RoslynTasksAssembly`, `CSharpCoreTargetsPath` and `VisualBasicCoreTargetsPath`.
The `roslyn-vendor` stage uses `NUGET_PACKAGES=/nuget` during restore and build;
localization property queries select `TargetFramework=net10.0`.

The package includes GraphitRoslyn.LICENSE and GraphitRoslyn.NOTICE. The recipe
copies the pinned attributions into runtime provenance. Source, patch and
attribution checksums remain in `upstream.json` and the Docker build.

The [producer smoke test](../../docs/scip-dotnet-vendor.md) exercises the
integration through real C#/VB projects and the official SCIP CLI. SDK diagnostic
witnesses, historical execution logs and resource proof frameworks are not
maintained inputs.
