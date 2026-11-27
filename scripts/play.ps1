param(
    [Parameter(Mandatory)][string]$Exe,
    [ValidateSet('series-s','series-x')][string]$Console = 'series-s',
    [ValidateSet('0','1','2')][string]$Relay = '2',
    [ValidateSet('diag','release')][string]$Build = 'diag',
    [switch]$Baseline,
    [hashtable]$More = @{}
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$victory = Join-Path $root "target\$Build\victory.exe"
if (!(Test-Path -LiteralPath $victory -PathType Leaf)) { throw "Build first: $victory" }
$game = (Resolve-Path -LiteralPath $Exe).Path
$settings = @{
    VICTORY_RELAY = $Relay
    VICTORY_LATE_WAIT = '3'
    VICTORY_LATE_WAIT_WRITES = '0'
    VICTORY_LATE_WAIT_LISTS = '32'
    VICTORY_WIDE_BACK = '1'
    VICTORY_TEXTURE_NO_WAIT = '0'
    VICTORY_KEEP_SNAPSHOTS = '0'
    VICTORY_CARD_BUFFERS_MIN_MB = '1'
    VICTORY_CARD_BUFFERS_MB = '1024'
    VICTORY_LOG = 'warn,victory_gpu=info,victory_loader=info'
}
if ($Relay -eq '0') { $settings.VICTORY_LATE_WAIT = '0' }
$port = if ($Baseline) { '0' } else { '1' }
foreach ($name in @('CONSTANTS','DESCRIPTORS','GUARD','JOURNAL')) {
    $settings["VICTORY_PORT_$name"] = $port
}
$settings.VICTORY_PORT_CBV = '0'
$settings.VICTORY_PORT_CBV_PLACE = '0'
foreach ($name in $More.Keys) { $settings[$name] = [string]$More[$name] }
$before = @{}
$keep = @('VICTORY_CONFIG','VICTORY_GAME_DIR','VICTORY_CACHE_DIR','VICTORY_DATA_DIR')
$reset = @(Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $keep } | Select-Object -ExpandProperty Name)
Push-Location -LiteralPath $root
try {
    foreach ($name in ($reset + @($settings.Keys))) {
        if ($before.ContainsKey($name)) { continue }
        $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, $settings[$name], 'Process')
    }
    # Rust tracing writes ordinary INFO messages to stderr. Keep the native
    # streams separate so Windows PowerShell does not turn them into exceptions.
    $stdout = Join-Path $root 'victory-run.log'
    $stderr = Join-Path $root 'victory-errors.log'
    Write-Host "Starting Victory. Logs: $stdout and $stderr"
    $arguments = @('run', ('"' + $game + '"'), '--execute', '--console', $Console)
    $process = Start-Process -FilePath $victory -ArgumentList $arguments -WorkingDirectory $root -NoNewWindow -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Victory exited with code $($process.ExitCode). See $stderr" }
} finally {
    foreach ($name in $before.Keys) { [Environment]::SetEnvironmentVariable($name, $before[$name], 'Process') }
    Pop-Location
}
