# ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
#
# Exercise the shipped bootstrap/DLL pair against separately downloaded CEF.
# Keep the resource hard links and AppContainer access grants used by real installs.

param(
    [Parameter(Mandatory)][ValidatePattern('^v[0-9]+\.[0-9]+$')][string] $Tag,
    [Parameter(Mandatory)][string[]] $CefVersions,
    [Parameter(Mandatory)][string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This check requires Windows' }
. "$PSScriptRoot/windows-browser-check.ps1"
. "$PSScriptRoot/windows-crash-capture.ps1"
$root = [IO.Path]::GetFullPath($OutputDirectory)
$diagnostics = Join-Path $root 'diagnostics'
New-Item -ItemType Directory -Force $diagnostics | Out-Null
Start-Transcript -Path (Join-Path $diagnostics 'session.log')
$originalData = $env:LOCALAPPDATA
try {
    $assetName = 'sabine-system-windows-x86_64.zip'
    gh release download $Tag --repo Lantharos/Sabine --pattern $assetName --pattern sabine-release.json --dir $root --clobber
    if ($LASTEXITCODE -ne 0) { throw 'Could not download the published system' }
    $manifest = Get-Content (Join-Path $root 'sabine-release.json') -Raw | ConvertFrom-Json
    $archive = Join-Path $root $assetName
    if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $manifest.artifacts.$assetName.sha256) {
        throw 'Published system checksum mismatch'
    }
    $system = Join-Path $root 'system'
    Expand-Archive $archive $system -Force
    $index = Invoke-RestMethod 'https://cef-builds.spotifycdn.com/index.json'
    $results = @()
    foreach ($requested in $CefVersions) {
        $version = $requested.Trim()
        $build = @($index.windows64.versions | Where-Object cef_version -EQ $version)
        if ($build.Count -ne 1) { throw "CEF version is not in the official index: $version" }
        $file = @($build[0].files | Where-Object type -EQ minimal)[0]
        $case = Join-Path $root $version
        New-Item -ItemType Directory -Force $case | Out-Null
        $download = Join-Path $case $file.name
        Invoke-WebRequest ("https://cef-builds.spotifycdn.com/" + [Uri]::EscapeDataString($file.name)) -OutFile $download
        if ((Get-FileHash $download -Algorithm SHA1).Hash -ne $file.sha1) {
            throw "CEF checksum mismatch: $version"
        }
        tar -xjf $download -C $case
        if ($LASTEXITCODE -ne 0) { throw "Could not extract CEF $version" }
        $runtime = Join-Path $case ($file.name -replace '\.tar\.bz2$', '')
        foreach ($name in @('icudtl.dat', 'chrome_100_percent.pak', 'chrome_200_percent.pak', 'resources.pak')) {
            $target = Join-Path $runtime "Release/$name"
            if (-not (Test-Path $target)) {
                New-Item -ItemType HardLink -Path $target -Target (Join-Path $runtime "Resources/$name") | Out-Null
            }
        }
        $env:LOCALAPPDATA = Join-Path $case 'data'
        $bin = Join-Path $env:LOCALAPPDATA 'Sabine/bin'
        $hostDirectory = Join-Path $bin "versions/$($manifest.version)"
        New-Item -ItemType Directory -Force $hostDirectory | Out-Null
        Copy-Item "$system/*" $hostDirectory -Recurse -Force
        @{ active = $manifest.version } | ConvertTo-Json | Set-Content (Join-Path $bin 'current.json')
        foreach ($directory in @($runtime, $hostDirectory)) {
            icacls $directory /grant '*S-1-15-2-2:(OI)(CI)(RX)' /T /Q | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "Could not prepare sandbox access: $directory" }
        }
        $result = [ordered]@{
            mode = 'published'
            cef = $version
            bootstrap = (Get-Item (Join-Path $hostDirectory 'sabine-host.exe')).VersionInfo.FileVersion
            library = (Get-Item (Join-Path $runtime 'Release/libcef.dll')).VersionInfo.FileVersion
            passed = $false
            error = $null
        }
        try {
            Test-SabineBrowser -RuntimeDirectory $runtime
            $result.passed = $true
        } catch {
            $result.error = $_.ToString()
            Write-Host "FAILED $version`: $_"
            try {
                Save-SabineHostCrash -HostDirectory $hostDirectory -RuntimeDirectory $runtime -OutputDirectory (Join-Path $diagnostics $version)
            } catch {
                Write-Host "Crash capture failed: $_"
            }
        }
        $results += [pscustomobject]$result
        if (-not $result.passed) {
            Copy-Item (Join-Path $runtime 'Release/bootstrap.exe') (Join-Path $hostDirectory 'sabine-host.exe') -Force
            Copy-Item (Join-Path $runtime 'Release/chrome_elf.dll') $hostDirectory -Force
            $matched = [ordered]@{
                mode = 'runtime-bootstrap'
                cef = $version
                bootstrap = (Get-Item (Join-Path $hostDirectory 'sabine-host.exe')).VersionInfo.FileVersion
                library = $result.library
                passed = $false
                error = $null
            }
            try {
                Test-SabineBrowser -RuntimeDirectory $runtime
                $matched.passed = $true
            } catch {
                $matched.error = $_.ToString()
                Write-Host "FAILED matching bootstrap $version`: $_"
            }
            $results += [pscustomobject]$matched
        }
        $results | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $diagnostics 'results.json')
    }
    Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber | ConvertTo-Json | Set-Content (Join-Path $diagnostics 'windows.json')
    Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion | ConvertTo-Json | Set-Content (Join-Path $diagnostics 'graphics.json')
    if ($results.Where({ -not $_.passed }).Count) { throw 'A published host/runtime combination failed; see results.json' }
} finally {
    $env:LOCALAPPDATA = $originalData
    Stop-Transcript
}
