function Save-SabineHostCrash([string] $HostDirectory, [string] $RuntimeDirectory, [string] $OutputDirectory) {
    $tools = Join-Path $OutputDirectory 'procdump'
    New-Item -ItemType Directory -Force $tools | Out-Null
    $archive = Join-Path $tools 'procdump.zip'
    Invoke-WebRequest 'https://download.sysinternals.com/files/Procdump.zip' -OutFile $archive
    Expand-Archive $archive $tools -Force
    $binaryDirectory = Join-Path $RuntimeDirectory 'Release'
    $resources = Join-Path $RuntimeDirectory 'Resources'
    $profile = Join-Path $OutputDirectory 'probe-profile'
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $tools 'procdump64.exe'))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.WorkingDirectory = $binaryDirectory
    $start.Environment['PATH'] = "$binaryDirectory;$env:PATH"
    foreach ($argument in @(
        '-accepteula', '-mm', '-e', '-n', '1', '-x', $OutputDirectory,
        (Join-Path $HostDirectory 'sabine-host.exe'),
        '--sabine-runtime-smoke-test',
        "--sabine-resources-dir-path=$resources",
        "--sabine-locales-dir-path=$(Join-Path $resources 'locales')",
        "--root-cache-path=$profile"
    )) { $start.ArgumentList.Add($argument) }
    $capture = [Diagnostics.Process]::Start($start)
    $output = $capture.StandardOutput.ReadToEndAsync()
    $errors = $capture.StandardError.ReadToEndAsync()
    try {
        if (-not $capture.WaitForExit(45000)) {
            $capture.Kill($true)
            $capture.WaitForExit()
        }
        $output.Result | Set-Content (Join-Path $OutputDirectory 'procdump.log')
        $errors.Result | Add-Content (Join-Path $OutputDirectory 'procdump.log')
    } finally {
        if (-not $capture.HasExited) { $capture.Kill($true); $capture.WaitForExit() }
        $capture.Dispose()
        Remove-Item $tools -Recurse -Force
        if (Test-Path $profile) { Remove-Item $profile -Recurse -Force }
    }
}
