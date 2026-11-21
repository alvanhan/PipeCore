param([string]$PrebuiltDirectory)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$version = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
if (!$version) { throw 'Could not read the version from Cargo.toml.' }

$builds = @(
    @{ Target='x86_64-pc-windows-gnu'; Platform='windows-x86_64'; Binary='pipecore.exe'; Extension='zip' }
    @{ Target='x86_64-unknown-linux-gnu'; Platform='linux-x86_64'; Binary='pipecore'; Extension='tar.gz' }
)

$dist = (New-Item -ItemType Directory -Force dist).FullName
$checksums = foreach ($build in $builds) {
    if ($PrebuiltDirectory) {
        $inputDir = Join-Path $PrebuiltDirectory $build.Platform
        $binary = Join-Path $inputDir $build.Binary
        $expected = ((Get-Content -LiteralPath (Join-Path $inputDir 'SHA256SUMS') -TotalCount 1) -split '\s+')[0]
        if ((Get-FileHash -LiteralPath $binary).Hash -ne $expected) { throw "Invalid prebuilt checksum: $binary" }
    } else {
        docker compose run --rm --no-deps rust-dev cargo build --locked --release --target $build.Target --bin pipecore | Out-Host
        if ($LASTEXITCODE) { throw "Build failed for $($build.Target)" }
        $binary = "target/$($build.Target)/release/$($build.Binary)"
    }

    $name = "pipecore-v$version-$($build.Platform)"
    $package = Join-Path $dist $name
    New-Item -ItemType Directory -Force $package | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $package $build.Binary) -Force
    Copy-Item -LiteralPath README.md,LICENSE,CHANGELOG.md,SECURITY.md,CONTRIBUTING.md,IMPLEMENTATION.md -Destination $package -Force
    foreach ($folder in @('AGENT','examples','schemas','docs')) {
        $destination = Join-Path $package $folder
        New-Item -ItemType Directory -Force $destination | Out-Null
        Copy-Item -Path "$folder/*" -Destination $destination -Recurse -Force
    }

    $archive = Join-Path $dist "$name.$($build.Extension)"
    if ($build.Extension -eq 'zip') {
        Compress-Archive -LiteralPath $package -DestinationPath $archive -Force
    } else {
        tar -czf $archive -C $dist $name
        if ($LASTEXITCODE) { throw 'Linux archive creation failed.' }
    }

    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))"
}
[IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS'), ($checksums -join "`n") + "`n")
$checksums
