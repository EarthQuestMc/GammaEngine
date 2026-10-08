<#
.SYNOPSIS
    Starts the GammaEngine test server prepared by setup.ps1.

.PARAMETER Java
    Java executable. Default: java on the PATH. Point it at a JDK 21 to test the phase 1 target,
    for example $HOME\.jdks\ms-21.0.12.1\bin\java.exe.

.PARAMETER Memory
    Heap size, used for both -Xms and -Xmx. Default: 4G.

.PARAMETER Gc
    Garbage collector: zgc or g1. Default: the JVM's own choice (Parallel for Java 8 on a server
    machine, G1 from Java 9). zgc needs Java 15 or later and is made generational on Java 21 and 22;
    from Java 23 it is generational by default and -XX:+ZGenerational is deprecated.

.PARAMETER GcLog
    Writes the JVM's GC log, with safepoint pauses, to logs/gc-<pid>.log. JMX rounds pauses to whole
    milliseconds; this log gives them to the hundredth.

.PARAMETER JvmArgs
    Extra JVM arguments, added after the others.

.PARAMETER NoConsole
    Starts without reading the console, for a server run in the background with no input attached.
#>
param(
    [string]$Dir = (Join-Path $PSScriptRoot '..\..\test-server'),
    [string]$Java = 'java',
    [string]$Memory = '4G',
    [ValidateSet('', 'zgc', 'g1')]
    [string]$Gc = '',
    [switch]$GcLog,
    [string[]]$JvmArgs = @(),
    [switch]$NoConsole
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Dir = (Resolve-Path $Dir).Path
if (-not (Test-Path (Join-Path $Dir 'server.jar'))) {
    throw "No server.jar in $Dir. Run tools/test-server/setup.ps1 first."
}

# java -version writes to stderr; go through cmd so PowerShell does not turn it into an error.
$versionLine = (& cmd /c "`"$Java`" -version 2>&1" | Select-Object -First 1).ToString()
if ($versionLine -notmatch 'version "(\d+)(?:\.(\d+))?') {
    throw "Cannot read the Java version from: $versionLine"
}
$major = [int]$Matches[1]
if ($major -eq 1) {
    $major = [int]$Matches[2]
}

$arguments = @("-Xms$Memory", "-Xmx$Memory")
if ($major -ge 9) {
    # The module system closes what Forge and the mods reach into: java9args.txt opens it again.
    $arguments += "@$(Join-Path $repo 'java9args.txt')"
}
if ($Gc -eq 'zgc') {
    if ($major -lt 15) {
        throw "ZGC needs Java 15 or later; this is Java $major."
    }
    $arguments += '-XX:+UseZGC'
    if ($major -le 22 -and $major -ge 21) {
        $arguments += '-XX:+ZGenerational'
    }
} elseif ($Gc -eq 'g1') {
    $arguments += '-XX:+UseG1GC'
}
if ($GcLog) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Dir 'logs') | Out-Null
    if ($major -ge 9) {
        $arguments += '-Xlog:gc,safepoint:file=logs/gc-%p.log:uptime,level,tags'
    } else {
        $arguments += @('-Xloggc:logs/gc-%p.log', '-XX:+PrintGCDetails', '-XX:+PrintGCApplicationStoppedTime')
    }
}
$arguments += $JvmArgs
$arguments += @('-jar', 'server.jar')
if ($NoConsole) {
    # Before nogui: FML's argument parser drops an option without a value when it comes last.
    $arguments += '--noconsole'
}
$arguments += 'nogui'

Write-Host "[GammaEngine] Java $major ($versionLine)"
Write-Host "[GammaEngine] $Java $($arguments -join ' ')"
Push-Location $Dir
try {
    & $Java @arguments
} finally {
    Pop-Location
}
