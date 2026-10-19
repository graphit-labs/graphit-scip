FROM rust:1.98.0-slim-bookworm AS graphit-tools
WORKDIR /tools
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --locked --release --bin scip-run --bin scip-entrypoint

FROM mcr.microsoft.com/dotnet/sdk:10.0.401-noble@sha256:e70cdb7f80b0348f5cb85f19a8f670fca061f033d57eed12fa003d58b0e06317 AS roslyn-vendor
ENV NUGET_PACKAGES=/nuget
WORKDIR /roslyn-source
RUN apt-get update && apt-get install -y --no-install-recommends curl patch ca-certificates && rm -rf /var/lib/apt/lists/*
COPY vendor/roslyn-msbuild /roslyn-correction
RUN curl -fsSL https://codeload.github.com/dotnet/roslyn/tar.gz/35d9211b841e7613c1d2f8f5af6d628ace696c4c -o /tmp/roslyn.tar.gz \
    && echo 'a479b751c9809aece5cdb7eec0bcc6eb1f75b4e927af29dd86c30287c2e6201c  /tmp/roslyn.tar.gz' | sha256sum -c - \
    && echo '715b0e4cce59f56a988dc1c56e7c6913e6cdad0dd99acf0950e5b1c6111f4cec  /roslyn-correction/fix-command-line-transport.patch' | sha256sum -c - \
    && tar -xzf /tmp/roslyn.tar.gz --strip-components=1 -C /roslyn-source \
    && patch --batch --fuzz=0 -p1 -i /roslyn-correction/fix-command-line-transport.patch \
    && echo '300ee9927c50baafd477fa8debbd2dcf5cef2e31f44cdc5c61d7cf76add2de26  global.json' | sha256sum -c - \
    && echo '23d6f7420d3ad24b0ae1de8beba349acc84c88ef9b98697cc67ede79f8e36e4b  src/Compilers/Core/MSBuildTask/ManagedToolTask.cs' | sha256sum -c - \
    && echo 'ce286619d22db508cee670416bb165a92a31738d4e2e2bc859de7d20d3ecd0fa  src/Workspaces/MSBuild/BuildHost/MSBuild/ProjectFile/ProjectInstanceReader.cs' | sha256sum -c - \
    && rm /tmp/roslyn.tar.gz \
    && mkdir /roslyn-receipts \
    && dotnet msbuild src/Compilers/Core/MSBuildTask/MSBuild/Microsoft.Build.Tasks.CodeAnalysis.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true -p:TargetFramework=net10.0 -getProperty:TargetFramework,AssemblyVersion,AssemblyName -getItem:ProjectReference,Compile,PackageReference > /roslyn-receipts/task-graph.json \
    && dotnet msbuild src/Workspaces/MSBuild/BuildHost/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true -p:TargetFramework=net10.0 -getProperty:TargetFramework,AssemblyVersion,AssemblyName -getItem:ProjectReference,Compile,PackageReference > /roslyn-receipts/host-graph.json \
    && dotnet msbuild src/Workspaces/MSBuild/Contracts/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true -p:TargetFramework=net10.0 -getProperty:TargetFramework,AssemblyVersion,AssemblyName -getItem:ProjectReference,Compile,PackageReference > /roslyn-receipts/contracts-graph.json
RUN dotnet restore src/Compilers/Core/MSBuildTask/MSBuild/Microsoft.Build.Tasks.CodeAnalysis.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true \
    && dotnet msbuild src/Compilers/Core/MSBuildTask/MSBuild/Microsoft.Build.Tasks.CodeAnalysis.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true -p:TargetFramework=net10.0 -getProperty:NuGetPackageRoot,RestorePackagesPath,GenerateSatelliteAssemblies,UsingToolXliff,EnableXlfLocalization,XlfLanguages,XliffTasksAssembly,NuGetPackageFolders > /roslyn-receipts/task-localization-properties.json \
    && cp artifacts/obj/Microsoft.Build.Tasks.CodeAnalysis/project.assets.json /roslyn-receipts/task-localization-assets.json \
    && dotnet restore src/Workspaces/MSBuild/BuildHost/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true \
    && dotnet msbuild src/Workspaces/MSBuild/BuildHost/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost.csproj -p:Configuration=Release -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true -p:TargetFramework=net10.0 -getProperty:NuGetPackageRoot,RestorePackagesPath,GenerateSatelliteAssemblies,UsingToolXliff,EnableXlfLocalization,XlfLanguages,XliffTasksAssembly,NuGetPackageFolders > /roslyn-receipts/host-localization-properties.json \
    && cp artifacts/obj/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/project.assets.json /roslyn-receipts/host-localization-assets.json \
    && test -f /nuget/microsoft.dotnet.xlifftasks/10.0.0-beta.26324.4/build/Microsoft.DotNet.XliffTasks.props \
    && test -f /nuget/microsoft.dotnet.xlifftasks/10.0.0-beta.26324.4/build/Microsoft.DotNet.XliffTasks.targets \
    && sha256sum /nuget/microsoft.dotnet.xlifftasks/10.0.0-beta.26324.4/build/Microsoft.DotNet.XliffTasks.props /nuget/microsoft.dotnet.xlifftasks/10.0.0-beta.26324.4/build/Microsoft.DotNet.XliffTasks.targets > /roslyn-receipts/localization-import-sha256.txt
RUN dotnet build src/Compilers/Core/MSBuildTask/MSBuild/Microsoft.Build.Tasks.CodeAnalysis.csproj -c Release -m:1 -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true \
    && dotnet build src/Workspaces/MSBuild/BuildHost/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost.csproj -c Release -m:1 -p:TargetFrameworks=net10.0 -p:NetCurrent=net10.0 -p:NetRoslynBuildHostNetCoreVersion=net10.0 -p:DotNetBuildSourceOnly=true -p:DotNetBuildTests=false -p:DotNetUseShippingVersions=true \
    && find artifacts/bin/Microsoft.Build.Tasks.CodeAnalysis/Release/net10.0 artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0 artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts/Release/net10.0 -type f -exec sha256sum {} + \
    && test "$(find artifacts/bin/Microsoft.Build.Tasks.CodeAnalysis/Release/net10.0 -type f | wc -l)" -eq 20 \
    && test "$(find artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0 -type f | wc -l)" -eq 20 \
    && test "$(find artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts/Release/net10.0 -type f | wc -l)" -eq 3 \
    && cmp artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts/Release/net10.0/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts.dll artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts.dll \
    && cmp artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts/Release/net10.0/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts.xml artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts.xml \
    && find artifacts/bin/Microsoft.Build.Tasks.CodeAnalysis/Release/net10.0 artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0 artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.Contracts/Release/net10.0 -type f -exec sha256sum {} + > /roslyn-receipts/source-output-sha256.txt

FROM mcr.microsoft.com/dotnet/sdk:10.0.401-noble@sha256:e70cdb7f80b0348f5cb85f19a8f670fca061f033d57eed12fa003d58b0e06317 AS dotnet-vendor
WORKDIR /vendor-source
RUN apt-get update && apt-get install -y --no-install-recommends curl patch ca-certificates && rm -rf /var/lib/apt/lists/*
COPY vendor/scip-dotnet /vendor-correction
COPY --from=roslyn-vendor /roslyn-source/artifacts/bin/Microsoft.Build.Tasks.CodeAnalysis/Release/net10.0 /vendor-source/GraphitRoslyn/Tasks
COPY --from=roslyn-vendor /roslyn-source/artifacts/bin/Microsoft.CodeAnalysis.Workspaces.MSBuild.BuildHost/Release/net10.0 /vendor-source/GraphitRoslyn/Host
COPY --from=roslyn-vendor /roslyn-receipts /vendorfeed/roslyn-source-receipts
COPY --from=roslyn-vendor /roslyn-correction/LICENSE /vendor-source/GraphitRoslyn.LICENSE
COPY --from=roslyn-vendor /roslyn-correction/NOTICE /vendor-source/GraphitRoslyn.NOTICE
RUN curl -fsSL https://codeload.github.com/sourcegraph/scip-dotnet/tar.gz/3e1f671e65692f517b0d35521fd1951c63939e3c -o /tmp/upstream.tar.gz \
    && echo '6ec527899eecb016ad1b7b3f69d87de6c4d7b31b3ea5202d6b9169e276eb912d  /tmp/upstream.tar.gz' | sha256sum -c - \
    && echo 'b986ef5eace3f7305cdf8a66da027bb1675662cf3dce579d74b76a56b689fbeb  /vendor-correction/fix-symbol-origins.patch' | sha256sum -c - \
    && tar -xzf /tmp/upstream.tar.gz --strip-components=1 -C /vendor-source \
    && patch --batch --fuzz=0 -p1 -i /vendor-correction/fix-symbol-origins.patch \
    && echo 'b851d59aad37fd323dffcb9531b59c2c45614ef348ca085b863d1395f1650a5e  scip.proto' | sha256sum -c - \
    && echo '108b634b6089cb3746a3c1f8b6e25602d90051a087d50817fdb119e53b4fba84  ScipDotnet/Scip.cs' | sha256sum -c - \
    && echo 'f63eff0ac9fe9a57fd05b06e7da4fe7ad02709420f57694c400aa0d3a84bb18b  ScipDotnet/ScipSymbol.cs' | sha256sum -c - \
    && echo 'f5390f45f6dec917dffe2c49dcc2c2f808db6d26adb8c66b97f605722b60ab13  ScipDotnet/ScipDocumentIndexer.cs' | sha256sum -c - \
    && echo 'ec14be4bbecc0c3ce4096d78efc965f43311c1a614b7c7555b671f2d8ccaf6e4  ScipDotnet/ScipProjectIndexer.cs' | sha256sum -c - \
    && echo '89f4217380c1c24ef3bc1653ece8e11438af084652596d426175696e3b6eaa70  ScipDotnet/IndexCommandHandler.cs' | sha256sum -c - \
    && echo '59839bdb1455a5a2d26769a8fcf2afbcd5225901d5a763618ab67c412a18c056  ScipDotnet/ScipCSharpSyntaxWalker.cs' | sha256sum -c - \
    && echo '07a67b6d7f808053e37a42d1d7fd6cff2905027090a65d30652bf02eb5dc3ad0  ScipDotnet/ScipVisualBasicSyntaxWalker.cs' | sha256sum -c - \
    && echo 'fc03b4d3f8e5188c5813b1a0c9726d454b3a0ac86a0eec8681781fada2cfe09a  ScipDotnet/ScipSymbolOrigins.cs' | sha256sum -c - \
    && echo '8c1dd5b52472af2cd46d6cbe243bdd97318894f203b064db1239b670d136a81a  ScipDotnet/ScipDotnet.csproj' | sha256sum -c - \
    && echo 'd0561718565550addf28fdbfcfdb245324bff85b3ca9b32fde231f1da1366969  ScipDotnet/Program.cs' | sha256sum -c - \
    && cp /vendor-correction/NOTICE /vendor-source/NOTICE \
    && rm /tmp/upstream.tar.gz
RUN echo 'ae48df11a335dc1a615f4f938b69cba73bcf4485c4f97af49b38efb0f216353b  /vendor-source/GraphitRoslyn.LICENSE' | sha256sum -c - \
    && echo '3c76292340eb805fcf66c9b2bd935cb688664a6a516a7dccbec5a423a957895d  /vendor-source/GraphitRoslyn.NOTICE' | sha256sum -c - \
    && echo '3fda3acb664b70d172e20d9cf6d36d14fbcee74684bec591821a62f966fbdc9a  /vendor-source/NOTICE' | sha256sum -c - \
    && echo 'a1e21e6913ed5e24be5feca0099ab7ad34966b8d36c59d2cb2c3b05a6db39457  /vendor-source/LICENSE' | sha256sum -c - \
    && dotnet pack ScipDotnet/ScipDotnet.csproj -c Release -p:TargetFrameworks=net10.0 -p:PackageVersion=0.2.14-graphit.2 -p:GraphitRoslynStage=/vendor-source/GraphitRoslyn -o /vendorfeed

FROM scratch AS vendor-package-proof
COPY --from=dotnet-vendor /vendorfeed/scip-dotnet.0.2.14-graphit.2.nupkg /scip-dotnet.0.2.14-graphit.2.nupkg
COPY --from=roslyn-vendor /roslyn-receipts /roslyn-source-receipts

FROM mcr.microsoft.com/dotnet/sdk:10.0.401-noble@sha256:e70cdb7f80b0348f5cb85f19a8f670fca061f033d57eed12fa003d58b0e06317
RUN apt-get update && apt-get install -y --no-install-recommends util-linux ca-certificates git curl && rm -rf /var/lib/apt/lists/*
COPY --from=dotnet-vendor /vendorfeed /vendorfeed
COPY --from=dotnet-vendor /vendorfeed/roslyn-source-receipts /opt/vendor-provenance/roslyn-source
COPY --from=dotnet-vendor /vendor-source/GraphitRoslyn.LICENSE /opt/vendor-provenance/roslyn-source/LICENSE
COPY --from=dotnet-vendor /vendor-source/GraphitRoslyn.NOTICE /opt/vendor-provenance/roslyn-source/NOTICE
COPY --from=dotnet-vendor /vendor-source/NOTICE /opt/vendor-provenance/scip-dotnet/NOTICE
COPY --from=dotnet-vendor /vendor-source/LICENSE /opt/vendor-provenance/scip-dotnet/LICENSE
COPY vendor/scip-dotnet/NuGet.Config /vendorfeed/NuGet.Config
RUN dotnet tool install scip-dotnet --configfile /vendorfeed/NuGet.Config --framework net10.0 --version 0.2.14-graphit.2 --tool-path /opt/scip-dotnet --no-cache \
    && rm -rf /vendorfeed
ENV PATH="/opt/scip-dotnet:${PATH}" GRAPHIT_SCIP_FAMILY=dotnet
COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index
COPY --from=graphit-tools /tools/target/release/scip-run /usr/local/bin/graphit-scip-run
RUN chmod +x /usr/local/bin/graphit-scip-index /usr/local/bin/graphit-scip-run
ENTRYPOINT ["/usr/local/bin/graphit-scip-index"]
