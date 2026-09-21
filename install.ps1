<#
.SYNOPSIS
    Install boxpigma on Windows.

.DESCRIPTION
    Works out which release asset this machine needs, downloads it, checks it against the
    release's SHA256SUMS, and unpacks boxpigma.exe.

    The OS architecture comes from RuntimeInformation rather than PROCESSOR_ARCHITECTURE, so
    an x64-emulated shell on ARM64 still gets the aarch64 build.

    This file is deliberately ASCII-only: Windows PowerShell 5.1 reads a .ps1 without a BOM
    as ANSI, and the `irm | iex` form decodes the response as Latin-1, so anything else shows
    up as mojibake (or fails to parse).

.EXAMPLE
    irm https://raw.githubusercontent.com/GBLMX/pigma/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Dir 'D:\tools\boxpigma' -AddToPath

.PARAMETER Version
    Release tag to install, or 'latest' (the default).

.PARAMETER Dir
    Where boxpigma.exe goes. Default: %LOCALAPPDATA%\Programs\boxpigma.

.PARAMETER AddToPath
    Append Dir to the user PATH so a new shell can just run `boxpigma`.

.PARAMETER Checksums
    URL or path of SHA256SUMS. Default: beside the asset in the same release.

.PARAMETER Mirror
    Replaces the https://github.com prefix, for networks that cannot reach it directly.

.PARAMETER DryRun
    Print what would happen and stop.
#>
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$Dir = '',
    [string]$Checksums = '',
    [string]$Repo = 'GBLMX/pigma',
    # $Host is an automatic variable in PowerShell, so this knob is called Mirror.
    [string]$Mirror = '',
    [switch]$AddToPath,
    [switch]$DryRun,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest draws a progress bar that costs more than the download itself.
$ProgressPreference = 'SilentlyContinue'

function Write-Log([string]$Message) { Write-Host $Message }

function Get-Architecture {
    try {
        $os = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        switch ("$os") {
            'X64' { return 'x86_64' }
            'Arm64' { return 'aarch64' }
            default { throw "unsupported CPU: $os - this project ships x86_64 and aarch64 builds" }
        }
    }
    catch [System.Management.Automation.MethodInvocationException] {
        throw 'cannot determine the CPU architecture on this PowerShell/.NET'
    }
}

$cpu = Get-Architecture
$target = "$cpu-pc-windows-msvc"
$asset = "boxpigma-$target.zip"

if (-not $Dir) { $Dir = Join-Path $env:LOCALAPPDATA 'Programs\boxpigma' }
$Dir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Dir)
$exe = Join-Path $Dir 'boxpigma.exe'

if (-not $Mirror) { $Mirror = if ($env:BOXPIGMA_GITHUB) { $env:BOXPIGMA_GITHUB } else { 'https://github.com' } }
if ($Version -eq 'latest') {
    $base = "$Mirror/$Repo/releases/latest/download"
}
else {
    $base = "$Mirror/$Repo/releases/download/$Version"
}
$assetUrl = "$base/$asset"
if (-not $Checksums) { $Checksums = "$base/SHA256SUMS" }

Write-Log "install.ps1: $Repo $Version"
Write-Log "  platform   Windows $cpu -> $target"
Write-Log "  asset      $assetUrl"
Write-Log "  install to $exe"
Write-Log "  verify     $Checksums"

if ($DryRun) {
    Write-Log '  (dry run: nothing downloaded or written)'
    return
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("boxpigma-install-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $asset
    Write-Log '  downloading...'
    Invoke-WebRequest -Uri $assetUrl -OutFile $zip -UseBasicParsing

    # Reading the checksums and checking them are separate steps: a release published before
    # SHA256SUMS existed still installs (unverified, and said so), but a checksum that is
    # present and does not match is a hard failure.
    $sums = $null
    try {
        # A local file is accepted as well as a URL, which is also how the checksum path can
        # be exercised without publishing a release.
        if (Test-Path -Path $Checksums -ErrorAction SilentlyContinue) {
            $sums = Get-Content -Path $Checksums -Raw
        }
        else {
            $sums = (Invoke-WebRequest -Uri $Checksums -UseBasicParsing).Content
        }
    }
    catch {
        $sums = $null
    }

    if ($null -eq $sums) {
        Write-Log "  checksum   unavailable ($Checksums) - installing unverified"
    }
    else {
        $pattern = "\s" + [regex]::Escape($asset) + "\s*$"
        $line = ($sums -split "`n" | Where-Object { $_ -match $pattern } | Select-Object -First 1)
        if (-not $line) { throw "SHA256SUMS has no entry for $asset" }
        $want = ($line -split '\s+')[0].ToLower()
        $got = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
        if ($want -ne $got) { throw "checksum mismatch for $asset (expected $want, got $got)" }
        Write-Log "  checksum   ok ($got)"
    }

    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $unpacked = Join-Path $tmp 'boxpigma.exe'
    if (-not (Test-Path $unpacked)) { throw 'the archive did not contain boxpigma.exe' }

    if ((Test-Path $exe) -and -not $Force -and $Version -ne 'latest') {
        $current = (& $exe --version 2>$null) -join ' '
        if ($current -match [regex]::Escape($Version.TrimStart('v'))) {
            Write-Log "  already    $exe is already $($Version.TrimStart('v')) - nothing to do (use -Force to reinstall)"
            return
        }
    }

    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    try {
        Copy-Item -Path $unpacked -Destination $exe -Force
    }
    catch {
        throw "cannot replace $exe - is boxpigma still running? ($($_.Exception.Message))"
    }
    Write-Log "  installed  $exe"

    if ($AddToPath) {
        $user = [Environment]::GetEnvironmentVariable('Path', 'User')
        $parts = @($user -split ';' | Where-Object { $_ })
        if ($parts -notcontains $Dir) {
            [Environment]::SetEnvironmentVariable('Path', (($parts + $Dir) -join ';'), 'User')
            Write-Log "  note       added $Dir to the user PATH - open a new terminal"
        }
    }
    else {
        $onPath = @($env:PATH -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ieq $Dir.TrimEnd('\')) })
        if (-not $onPath) {
            Write-Log "  note       $Dir is not in PATH - add it, or re-run with -AddToPath"
        }
    }
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
