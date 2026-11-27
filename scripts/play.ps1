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
# Match the working 14b legacy Relay settings without FSR or timer overrides.
$settings = @{
    VICTORY_RELAY = $Relay
    VICTORY_LATE_WAIT = $(if ($Relay -eq '0') { '0' } else { '3' })
    VICTORY_LATE_WAIT_LISTS = '32'
    VICTORY_RELAY_NEXT = '0'
    VICTORY_RELAY_COPY = '0'
    VICTORY_KEEP_SNAPSHOTS = '0'
    VICTORY_ASYNC_SUBMIT = $(if ($Relay -eq '0') { '1' } else { '0' })
    VICTORY_THIN = '0'
    VICTORY_UPLOAD_AHEAD = '1'
    VICTORY_INLINE_READBACK = '0'
    VICTORY_CARD_STEPS = '0'
    VICTORY_TEXTURE_NO_WAIT = '0'
    VICTORY_D3D12_DRED = 'light'
    VICTORY_D3D12_DEBUG = '0'
    VICTORY_AMBIENT = '0.8'
    VICTORY_SHOW_AFTER = '1'
    VICTORY_PLANES = '1'
    VICTORY_SKIP_PIPELINES = 'none'
    VICTORY_LOG = 'warn,victory_gpu=info,victory_loader=info'
}
$port = if ($Baseline) { '0' } else { '1' }
foreach ($name in @('CONSTANTS','DESCRIPTORS','GUARD','JOURNAL','AMD_TEXTURE_BUDGET','AMD_BATCH_POOL','AMD_PROBE_FILTER')) {
    $settings["VICTORY_PORT_$name"] = $port
}
$settings.VICTORY_PORT_TEXTURE_PRELUDE = if ($Relay -eq '0' -and !$Baseline) { '1' } else { '0' }
$settings.VICTORY_PORT_CBV = '0'
$settings.VICTORY_PORT_CBV_PLACE = '0'
$settings.VICTORY_PORT_CBV_RAW16 = '0'
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
