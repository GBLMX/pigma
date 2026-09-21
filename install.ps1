<#
.SYNOPSIS
    在 Windows 上安装 boxpigma。

.DESCRIPTION
    安装布局和 install.sh 一致（版本化安装 + 原子切换）：

        <Dir>\releases\<版本>-<目标平台>\boxpigma.exe   每个版本各占一个目录，互不覆盖
        <Dir>\current                                  目录联接（mklink /J），指向当前版本
        <Dir>\install.lock                             纯文本，记录当前版本、安装时间与版本历史

    流程：下载 -> 校验 SHA256SUMS -> 解压到 releases 下的临时目录 -> 改名成正式版本目录
    -> 最后一步才切换 current。校验失败不会切换；切换失败会还原旧链接并以非 0 退出。

    本文件以 UTF-8 (BOM) 保存：Windows PowerShell 5.1 读没有 BOM 的 .ps1 会按 ANSI 解码，
    中文会变成乱码。请不要去掉 BOM。

    CPU 架构取自 RuntimeInformation 而不是 PROCESSOR_ARCHITECTURE，这样 ARM64 上被 x64
    模拟的 shell 也能拿到 aarch64 产物。

.EXAMPLE
    irm https://raw.githubusercontent.com/GBLMX/pigma/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Dir 'D:\tools\boxpigma' -AddToPath

.EXAMPLE
    .\install.ps1 -Dir 'D:\tools\boxpigma' -Rollback

.PARAMETER Version
    要安装的 release tag，或 'latest'（默认）。

.PARAMETER Dir
    安装目录。默认 %LOCALAPPDATA%\Programs\boxpigma。

.PARAMETER AddToPath
    把 <Dir>\current 追加到用户 PATH，新开的终端就能直接跑 boxpigma。

.PARAMETER Checksums
    SHA256SUMS 的 URL 或本地路径。默认取同一 release 里那份。

.PARAMETER Mirror
    替换 https://github.com 前缀，给连不上 GitHub 的网络用。

.PARAMETER Rollback
    把 current 切回上一个版本。

.PARAMETER DryRun
    只打印计划，不下载不写入。

.PARAMETER Force
    同一个版本也重新下载安装。
#>
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$Dir = '',
    [string]$Checksums = '',
    [string]$Repo = 'GBLMX/pigma',
    # $Host 是 PowerShell 的自动变量，所以这个开关叫 Mirror。
    [string]$Mirror = '',
    [switch]$AddToPath,
    [switch]$Rollback,
    [switch]$DryRun,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
# Invoke-WebRequest 的进度条比下载本身还贵。
$ProgressPreference = 'SilentlyContinue'

# 保留多少个版本目录（含当前版本）；current 指向的那个无论如何都留着。
$Keep = 3
$Bin = 'boxpigma.exe'

function Write-Log([string]$Message) { Write-Host $Message }

function Fail([string]$Message) {
    [Console]::Error.WriteLine("install.ps1: $Message")
    exit 1
}

function Get-Architecture {
    try {
        $os = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        switch ("$os") {
            'X64' { return 'x86_64' }
            'Arm64' { return 'aarch64' }
            default { Fail "不支持的 CPU：$os —— 本项目只发布 x86_64 与 aarch64 产物" }
        }
    }
    catch [System.Management.Automation.MethodInvocationException] {
        Fail '在这台 PowerShell/.NET 上判断不出 CPU 架构'
    }
}

# ---------------------------------------------------------------- 布局相关工具 ----

function Test-IsLink([string]$Path) {
    # 目录联接/符号链接都带 ReparsePoint 属性；断链时 Test-Path 也是 False，只能这样认。
    try { $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop }
    catch { return $false }
    return [bool]($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint)
}

function Get-CurrentTarget {
    # current 指向的目录名；没有链接就返回空串。
    if (-not (Test-IsLink $current)) { return '' }
    $item = Get-Item -LiteralPath $current -Force -ErrorAction SilentlyContinue
    if ($null -eq $item) { return '' }
    $t = $item.Target
    if ($t -is [array]) { $t = $t[0] }
    if (-not $t) { return '' }
    return (Split-Path -Leaf ([string]$t).TrimEnd('\'))
}

function Remove-Link([string]$Path) {
    # 只删链接本身：cmd 的 rmdir 遇到联接/符号链接删的就是那个 reparse point，
    # 绝不会像 Remove-Item -Recurse 那样把目标目录里的东西一起删掉。
    # （注意括号：命令后面直接接 -or 会被 PowerShell 当成参数。）
    if (-not (Test-Path -LiteralPath $Path) -and -not (Test-IsLink $Path)) { return }
    & cmd.exe /c rmdir "$Path" | Out-Null
    if ((Test-Path -LiteralPath $Path) -or (Test-IsLink $Path)) {
        try { [System.IO.Directory]::Delete($Path, $false) } catch { }
    }
}

function New-Link([string]$Path, [string]$Target) {
    # 先试目录联接（mklink /J，不需要管理员权限或开发者模式），失败再退到符号链接。
    & cmd.exe /c mklink /J "$Path" "$Target" | Out-Null
    if (Test-Path -LiteralPath $Path) { return $true }
    try {
        New-Item -ItemType SymbolicLink -Path $Path -Target $Target -ErrorAction Stop | Out-Null
        return $true
    }
    catch {
        Write-Log "  警告       建不出 $Path -> $Target（$($_.Exception.Message)）"
        return $false
    }
}

function Set-Current([string]$Name) {
    # 把 current 换到 releases\<Name>：新链接先在旁边建好，再删旧链接、把它改名过去。
    $newLink = Join-Path $Dir '.current.new'
    Remove-Link $newLink
    if (-not (New-Link $newLink (Join-Path $releases $Name))) { return $false }
    Remove-Link $current
    try {
        [System.IO.Directory]::Move($newLink, $current)
        return $true
    }
    catch {
        # 改名失败就退化成直接建链接（多一次窗口，但结果一样）。
        if (New-Link $current (Join-Path $releases $Name)) { return $true }
        return $false
    }
}

function Restore-Current([string]$Name) {
    Remove-Link $current
    return (New-Link $current (Join-Path $releases $Name))
}

function Get-History {
    # install.lock 里的版本历史，旧 -> 新。
    if (-not (Test-Path -LiteralPath $lockFile)) { return @() }
    $line = Get-Content -LiteralPath $lockFile -ErrorAction SilentlyContinue |
        Where-Object { $_ -like 'history=*' } | Select-Object -First 1
    if (-not $line) { return @() }
    return @(($line.Substring(8) -split '\s+') | Where-Object { $_ })
}

function Get-VersionDirs {
    # releases 下的版本目录，新 -> 旧。先认 install.lock 的记录（权威），
    # 不在记录里的（比如 lock 被删过）再按目录修改时间排。
    $hist = @(Get-History)
    $newFirst = @()
    if ($hist.Count -gt 0) { $newFirst = @($hist[($hist.Count - 1)..0]) }

    $ordered = New-Object System.Collections.Generic.List[string]
    foreach ($h in $newFirst) {
        $p = Join-Path $releases $h
        if ((Test-Path -LiteralPath $p -PathType Container) -and -not (Test-IsLink $p) -and
            -not $ordered.Contains($h)) {
            $ordered.Add($h)
        }
    }
    if (Test-Path -LiteralPath $releases -PathType Container) {
        $rest = Get-ChildItem -LiteralPath $releases -Force -ErrorAction SilentlyContinue |
            Where-Object {
                $_.PSIsContainer -and
                -not ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -and
                -not $_.Name.StartsWith('.') -and
                -not $ordered.Contains($_.Name)
            } |
            Sort-Object LastWriteTime -Descending |
            ForEach-Object { $_.Name }
        foreach ($name in @($rest)) { $ordered.Add($name) }
    }
    return @($ordered)
}

function Get-NewHistory([string]$Rel) {
    # 版本历史（旧 -> 新）= 现有目录（去掉重复和已删的）+ 本次装的版本。
    $out = @()
    foreach ($h in (Get-VersionDirs)) {
        if ($h -ne $Rel) { $out = , $h + $out }
    }
    if ($out.Count -gt 0) { return (($out -join ' ') + ' ' + $Rel) }
    return $Rel
}

function Write-Lock([string]$Rel, [string]$Num, [string]$Prev) {
    $lines = @(
        "version=$Num",
        "target=$target",
        "dir=releases\$Rel",
        ("installed_at=" + [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss') + 'Z'),
        "previous=$Prev",
        ("history=" + (Get-NewHistory $Rel))
    )
    try { Set-Content -LiteralPath $lockFile -Value $lines -Encoding ASCII -ErrorAction Stop }
    catch { Fail "写不了 $lockFile（权限不足？）" }
}

function Remove-OldVersions([string]$KeepRel) {
    # 只保留最近 Keep 个版本目录；current 指向的、以及刚装好的那个永不删。
    $cur = Get-CurrentTarget
    $n = 0
    foreach ($name in (Get-VersionDirs)) {
        $n++
        if ($n -le $Keep) { continue }
        if ($name -eq $KeepRel) { continue }
        if ($name -eq $cur) { continue }
        $path = Join-Path $releases $name
        try {
            Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop
            Write-Log "  清理       旧版本 $path"
        }
        catch { Write-Log "  警告       旧版本 $path 删不掉，先留着：$($_.Exception.Message)" }
    }
}

function Get-ExeVersion([string]$Path) {
    # 运行二进制取版本号：`boxpigma 1.4.0` -> `1.4.0`
    try { $out = (& $Path --version 2>$null) -join ' ' }
    catch { return '' }
    if ($out -match '([0-9][0-9A-Za-z.+\-]*)') { return $Matches[1] }
    return ''
}

function Show-Result([string]$Rel, [string]$Num, [string]$Prev) {
    Write-Log "  已安装     $releases\$Rel\$Bin"
    Write-Log "  版本       $Num"
    Write-Log "  current    $current -> releases\$Rel"
    Write-Log "  可执行     $current\$Bin"
    if ($Prev -eq $Rel) {
        Write-Log '  上一个版本 （没有变化，current 本来就指向它）'
    }
    elseif ($Prev) {
        Write-Log "  上一个版本 $Prev"
    }
    else {
        Write-Log '  上一个版本 （无）'
    }
    Write-Log "  回滚命令   .\install.ps1 -Dir '$Dir' -Rollback"
}

function Update-Path {
    # PATH 与提示统统指向 <Dir>\current。
    if ($AddToPath) {
        # 老布局残留的 <Dir> 条目顺手换掉。
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $parts = @($userPath -split ';' | Where-Object { $_ })
        $wanted = $current.TrimEnd('\')
        $hasWanted = @($parts | Where-Object { $_.TrimEnd('\') -ieq $wanted }).Count -gt 0
        $hasOld = @($parts | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') }).Count -gt 0
        if (-not $hasWanted -or $hasOld) {
            $kept = @($parts | Where-Object {
                    $_.TrimEnd('\') -ine $wanted -and $_.TrimEnd('\') -ine $Dir.TrimEnd('\')
                })
            [Environment]::SetEnvironmentVariable('Path', (($kept + $wanted) -join ';'), 'User')
            if ($hasOld) {
                Write-Log "  提示       用户 PATH 里的 $Dir 已换成 $current —— 打开新终端生效"
            }
            else {
                Write-Log "  提示       已把 $current 加进用户 PATH —— 打开新终端就能直接跑 boxpigma"
            }
        }
    }
    else {
        $onPath = @($env:PATH -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ieq $current.TrimEnd('\')) })
        if (-not $onPath) {
            Write-Log "  提示       $current 不在 PATH 里，加上它（或重跑本脚本时带上 -AddToPath）：
               `$env:PATH = '$current;' + `$env:PATH"
        }
    }
}

function Complete-Install([string]$Rel, [string]$Num) {
    # 切换 current、写锁、清理旧版本、打印结论。
    $prev = Get-CurrentTarget
    if (-not (Set-Current $Rel)) {
        if ($prev) {
            if (Restore-Current $prev) {
                Write-Log "  回退       current 已还原成 releases\$prev"
            }
            else {
                Write-Log "  警告       旧链接也没能还原，请手动把 $current 指向 releases\$prev"
            }
        }
        Fail "切换 current 到 releases\$Rel 失败 —— 安装中止，current 未被改动"
    }
    Write-Lock $Rel $Num $prev
    Remove-OldVersions $Rel
    Show-Result $Rel $Num $prev
}

# ---------------------------------------------------------------- 基本参数 ----

$cpu = Get-Architecture
$target = "$cpu-pc-windows-msvc"
$asset = "boxpigma-$target.zip"

if (-not $Dir) {
    if (-not $env:LOCALAPPDATA) { Fail '拿不到 %LOCALAPPDATA%，请用 -Dir 指定安装目录' }
    $Dir = Join-Path $env:LOCALAPPDATA 'Programs\boxpigma'
}
$Dir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Dir)
$releases = Join-Path $Dir 'releases'
$current = Join-Path $Dir 'current'
$lockFile = Join-Path $Dir 'install.lock'

if (-not $Mirror) { $Mirror = if ($env:BOXPIGMA_GITHUB) { $env:BOXPIGMA_GITHUB } else { 'https://github.com' } }
if ($Version -eq 'latest') {
    $base = "$Mirror/$Repo/releases/latest/download"
}
else {
    $base = "$Mirror/$Repo/releases/download/$Version"
}
$assetUrl = "$base/$asset"
if (-not $Checksums) { $Checksums = "$base/SHA256SUMS" }

# 显式版本号现在就算得出来；latest 要等下载下来问二进制自己。
$versionNum = ''
if ($Version -ne 'latest') {
    $versionNum = $Version.TrimStart('v')
    if ($versionNum -notmatch '^[0-9]') {
        Fail "版本号看着不对：$Version（期望形如 v1.4.0 或 1.4.0）"
    }
}

# ---------------------------------------------------------------- 回滚 ----

if ($Rollback) {
    if (-not (Test-Path -LiteralPath $releases -PathType Container)) {
        Fail "没有可回滚的东西：$releases 不存在（还没有用本脚本安装过）"
    }
    $cur = Get-CurrentTarget
    $targetRel = ''
    foreach ($name in (Get-VersionDirs)) {
        if ($name -ne $cur) { $targetRel = $name; break }
    }
    if (-not $targetRel) {
        $only = if ($cur) { $cur } else { '（没有 current 链接）' }
        Fail "没有可回滚的版本：$releases 里只有 $only"
    }

    if ($DryRun) {
        Write-Log 'install.ps1: 回滚（dry-run）'
        Write-Log "  current    $(if ($cur) { $cur } else { '（无）' }) -> releases\$targetRel"
        Write-Log '  (dry-run：没有改动任何东西)'
        return
    }

    if (-not (Set-Current $targetRel)) {
        if ($cur) { Restore-Current $cur | Out-Null }
        Fail "回滚失败：未能把 $current 指向 releases\$targetRel"
    }
    $ver = $targetRel -replace ("-" + [regex]::Escape($target) + '$'), ''
    Write-Lock $targetRel $ver $cur
    Write-Log 'install.ps1: 已回滚'
    Write-Log "  版本       $ver"
    Write-Log "  current    $current -> releases\$targetRel"
    Write-Log "  可执行     $current\$Bin"
    Write-Log "  上一个版本 $(if ($cur) { $cur } else { '（无）' })"
    Write-Log "  回滚命令   .\install.ps1 -Dir '$Dir' -Rollback"
    return
}

# ---------------------------------------------------------------- 安装 ----

Write-Log "install.ps1: $Repo $Version"
Write-Log "  platform   Windows $cpu -> $target"
Write-Log "  asset      $assetUrl"
if ($versionNum) {
    Write-Log "  install    $releases\$versionNum-$target\$Bin"
}
else {
    Write-Log "  install    $releases\<下载后确定>-$target\$Bin"
}
Write-Log "  current    $current"
Write-Log "  verify     $Checksums"

if ((Test-Path -LiteralPath $Dir) -and -not (Test-Path -LiteralPath $Dir -PathType Container)) {
    Fail "$Dir 已经存在，而且不是目录"
}
if ((Test-Path -LiteralPath $current) -and -not (Test-IsLink $current)) {
    Fail "$current 已经存在，而且不是目录联接或符号链接 —— 请先删掉它，或换一个 -Dir"
}

if ($DryRun) {
    Write-Log '  (dry-run：没有下载，也没有写入)'
    return
}

try { New-Item -ItemType Directory -Force -Path $releases -ErrorAction Stop | Out-Null }
catch { Fail "创建不了版本目录 $releases（权限不足？）" }

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("boxpigma-install-" + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $tmp | Out-Null
$staging = Join-Path $releases ('.staging-' + [guid]::NewGuid().ToString('n'))
$old = Join-Path $releases ('.old-' + [guid]::NewGuid().ToString('n'))
try {
    $zip = Join-Path $tmp $asset

    # ------------------------------------------------------------------ 校验和 ----
    # 读校验和与校验是两步：早于 SHA256SUMS 的 release 仍然能装（不校验，但要说明）；
    # 校验和存在却对不上则是硬错误，必须中止。
    $sums = $null
    if (Test-Path -LiteralPath $Checksums -ErrorAction SilentlyContinue) {
        # 本地文件也认，这样不发布 release 也能试校验这条路。
        $sums = Get-Content -LiteralPath $Checksums -Raw
    }
    else {
        try {
            $response = Invoke-WebRequest -Uri $Checksums -UseBasicParsing -ErrorAction Stop
            $sums = $response.Content
            # GitHub 给 SHA256SUMS 的 Content-Type 是 application/octet-stream，
            # PowerShell 7 这时会把 Content 给成 byte[]，得自己按 UTF-8 解。
            if ($sums -is [byte[]]) { $sums = [System.Text.Encoding]::UTF8.GetString($sums) }
        }
        catch { $sums = $null }
    }

    $want = ''
    if ($null -eq $sums -or "$sums".Trim() -eq '') {
        Write-Log "  校验       拿不到 $Checksums —— 这次没有校验就安装"
    }
    else {
        $pattern = "\s" + [regex]::Escape($asset) + "\s*$"
        $line = ($sums -split "`n" | Where-Object { $_ -match $pattern } | Select-Object -First 1)
        if (-not $line) { Fail "SHA256SUMS 里没有 $asset 的记录（$Checksums）" }
        $want = ($line -split '\s+')[0].ToLower()
    }

    # ------------------------------------------------------------------ 复用 ----
    # 已经装过同一个版本：目录里的记录和 release 的 SHA256SUMS 对得上就不用再下载。
    $match = ''
    if ($want -and -not $Force) {
        if ($versionNum) {
            $cand = "$versionNum-$target"
            $record = Join-Path $releases "$cand\.archive.sha256"
            $exePath = Join-Path $releases "$cand\$Bin"
            if ((Test-Path -LiteralPath $exePath) -and (Test-Path -LiteralPath $record)) {
                if ((Get-Content -LiteralPath $record -Raw).Trim() -eq $want) { $match = $cand }
            }
        }
        else {
            # -Version latest：按记录反查这堆目录里有没有就是最新版的那个。
            foreach ($name in (Get-VersionDirs)) {
                $record = Join-Path $releases "$name\.archive.sha256"
                if ((Test-Path -LiteralPath (Join-Path $releases "$name\$Bin")) -and (Test-Path -LiteralPath $record)) {
                    if ((Get-Content -LiteralPath $record -Raw).Trim() -eq $want) { $match = $name; break }
                }
            }
        }
    }
    # 没有校验和可比对（老 release 没发 SHA256SUMS）时，退回问二进制自己。
    if (-not $match -and -not $Force -and $versionNum) {
        $exePath = Join-Path $releases "$versionNum-$target\$Bin"
        if ((Test-Path -LiteralPath $exePath) -and ((Get-ExeVersion $exePath) -eq $versionNum)) {
            $match = "$versionNum-$target"
        }
    }

    if ($match) {
        Write-Log "  已存在     $releases\$match 校验一致，跳过下载（-Force 可强制重装）"
        $matchedVer = $match -replace ("-" + [regex]::Escape($target) + '$'), ''
        Complete-Install $match $matchedVer
        Update-Path
        return
    }

    # ------------------------------------------------------------------ 下载 ----
    Write-Log '  下载中...'
    try {
        Invoke-WebRequest -Uri $assetUrl -OutFile $zip -UseBasicParsing -ErrorAction Stop
    }
    catch {
        $response = $_.Exception.Response
        $code = 0
        if ($response) {
            try { $code = [int]$response.StatusCode } catch { $code = 0 }
        }
        if ($code -eq 404) {
            Fail "release $Version 里没有 $asset（$assetUrl）—— 这个平台可能没有发布产物，或 tag 写错了"
        }
        elseif ($code -eq 0) {
            Fail "下载失败：$assetUrl —— 网络不通？可以用 -Mirror 指向镜像（$($_.Exception.Message)）"
        }
        else {
            Fail "下载失败：$assetUrl（HTTP $code）"
        }
    }

    # ------------------------------------------------------------------ 校验 ----
    if ($want) {
        $got = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
        if ($want -ne $got) {
            Fail "校验失败：$asset 的 SHA256 对不上（期望 $want，实际 $got）—— 已中止，current 没有被改动"
        }
        Write-Log "  校验       ok（$got）"
    }

    # ------------------------------------------------------------------ 解压 ----
    # 先解到 releases 下的临时目录，装完再改名成正式目录，避免出现半个版本的中间态。
    New-Item -ItemType Directory -Path $staging | Out-Null
    try { Expand-Archive -LiteralPath $zip -DestinationPath $staging -Force -ErrorAction Stop }
    catch { Fail "解压失败：$asset（$($_.Exception.Message)）" }
    $unpacked = Join-Path $staging $Bin
    if (-not (Test-Path -LiteralPath $unpacked)) { Fail "压缩包里没有 $Bin" }

    if (-not $versionNum) {
        # -Version latest：这时候才知道装的是哪个版本。
        $versionNum = Get-ExeVersion $unpacked
        if (-not $versionNum -or $versionNum -notmatch '^[0-9]') {
            Fail "$Bin --version 没有给出可用版本号 —— 请显式指定 -Version <tag>"
        }
        Write-Log "  版本       latest = $versionNum"
    }

    $relName = "$versionNum-$target"
    $versionDir = Join-Path $releases $relName
    if ($want) {
        # 记下压缩包的哈希，下次同一个版本就能直接跳过下载。
        Set-Content -LiteralPath (Join-Path $staging '.archive.sha256') -Value $want -Encoding ASCII
    }

    if (Test-Path -LiteralPath $versionDir) {
        try { [System.IO.Directory]::Move($versionDir, $old) }
        catch { Fail "挪不动旧目录 $versionDir（有进程在用？）" }
    }
    try { [System.IO.Directory]::Move($staging, $versionDir) }
    catch {
        if (Test-Path -LiteralPath $old) {
            try { [System.IO.Directory]::Move($old, $versionDir) } catch { }
        }
        Fail "装不进 $versionDir"
    }
    if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Recurse -Force -ErrorAction SilentlyContinue }

    Complete-Install $relName $versionNum
    Update-Path
}
finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue }
    if (Test-Path -LiteralPath $old) { Remove-Item -LiteralPath $old -Recurse -Force -ErrorAction SilentlyContinue }
}
