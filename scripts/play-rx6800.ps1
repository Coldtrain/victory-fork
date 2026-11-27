param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Parallel,
    [switch]$RNext,
    [switch]$CoolFoxResidency,
    [switch]$CoolFoxThin,
    [hashtable]$More = @{}
)

$root = Split-Path -Parent $PSScriptRoot
$victory = Join-Path $root 'target\diag\victory.exe'
if (-not (Test-Path -LiteralPath $victory -PathType Leaf)) {
    throw "Build the diagnostics executable first: $victory"
}
$game = (Resolve-Path -LiteralPath $Exe -ErrorAction Stop).Path

$settings = @{
    # Port of the NVIDIA fork's resolution-only settings; no GPU vendor changes.
    #VICTORY_RENDER_SCALE = '0.20'
    VICTORY_OWN_UPSCALER = '1'
    VICTORY_RELAY = '0'
    VICTORY_LATE_WAIT = '0'
    VICTORY_RELAY_COPY = '0'
    VICTORY_RELAY_NEXT = '0'
    VICTORY_ASYNC_SUBMIT = '1'
    VICTORY_THIN = '0'
    VICTORY_UPLOAD_AHEAD = '1'
    VICTORY_CARD_STEPS = '0'
    VICTORY_TEXTURE_NO_WAIT = '0'
    VICTORY_D3D12_DRED = 'light'
    VICTORY_D3D12_DEBUG = '0'
    VICTORY_LOG = 'warn,victory_gpu::host=info,victory_gpu::relay_next=info,victory_gpu::queue::behind=info'
    VICTORY_AMBIENT = '0.8'
    VICTORY_SHOW_AFTER = '1'
    VICTORY_SKIP_PIPELINES = 'none'
    # Belobug renderer optimizations; CBV remains off due to artifacts.
    VICTORY_PORT_CONSTANTS = '1'
    VICTORY_PORT_DESCRIPTORS = '1'
    VICTORY_PORT_CBV = '0'
    VICTORY_PORT_CBV_PLACE = '0'
    VICTORY_PORT_CBV_RAW16 = '0'
    VICTORY_PORT_AMD_TEXTURE_BUDGET = '1' # Sprint 12a: adaptive mip and VRAM policy
    VICTORY_PORT_AMD_BATCH_POOL = '1' # Sprint 12b: shared indirect argument buffers
    VICTORY_PORT_AMD_PROBE_FILTER = '1' # Sprint 12c: filter suspect magenta ambient probes
    VICTORY_PORT_GUARD = '1'
    VICTORY_PORT_JOURNAL = '1'
    VICTORY_PORT_TEXTURE_PRELUDE = '1' # Belobug submission-time uploads (Relay 0)
}

if ($RNext) {
    $settings.VICTORY_PORT_TEXTURE_PRELUDE = '0'
    $settings.VICTORY_RELAY_NEXT = '2'
    $settings.VICTORY_RELAY_NEXT_SNAPSHOTS = '1'
    $settings.VICTORY_RELAY_NEXT_MERGE = '0'
    $settings.VICTORY_RELAY_NEXT_RESIDENT = '1'
    $settings.VICTORY_RELAY_NEXT_READBACK_KB = '16'
    $settings.VICTORY_RELAY_NEXT_PACKED_READBACK = '1'
    $settings.VICTORY_RELAY_NEXT_PACKED_MB = '96'
    $settings.VICTORY_RELAY_NEXT_SCHEDULER = '1'
    $settings.VICTORY_RELAY_NEXT_QUEUE_DEPTH = '8'
    $settings.VICTORY_RELAY_NEXT_JOIN_PARTS = '7'
    $settings.VICTORY_RELAY_NEXT_DEPENDENCY_BATCHES = '1'
    $settings.VICTORY_RELAY_NEXT_CROSS_QUEUE = '1'
    $settings.VICTORY_RELAY_NEXT_SPARSE_HASH = '1'
    $settings.VICTORY_RELAY_NEXT_INDEXED_REFRESH = '1'
    $settings.VICTORY_RELAY_NEXT_REFRESH_CHUNK = '8'
    $settings.VICTORY_RELAY_NEXT_TICKET_OVERLAP = '1'
    $settings.VICTORY_RELAY_NEXT_OVERLAP_COMPARE = '1'
    $settings.VICTORY_RELAY_NEXT_INDEPENDENT_FLIGHTS = '1'
    $settings.VICTORY_RELAY_NEXT_ORDERED_SIGNALS = '1'
    $settings.VICTORY_RELAY_NEXT_PERSISTENT_FLIGHTS = '1'
    $settings.VICTORY_RELAY_NEXT_MAX_FLIGHTS = '4'
    $settings.VICTORY_RELAY_NEXT_MERGE_WORKERS = '4'
    $settings.VICTORY_RELAY_NEXT_TRACE = '0'
}

if ($Parallel) {
    $settings.VICTORY_ASYNC_SUBMIT = '1'
    $settings.VICTORY_THIN = '1'
    $settings.VICTORY_THIN_THREADS = '8'
    $settings.VICTORY_UPLOAD_AHEAD = '1'
    $settings.VICTORY_UPLOAD_WORKERS = '8'
    $settings.VICTORY_INLINE_READBACK = '1'
    $settings.VICTORY_LOG = 'warn,victory_gpu::host=info,victory_gpu::relay_next=info,victory_gpu::list::thin=info,victory_gpu::queue::behind=info,victory_gpu::texture::ahead=info'
}
# Configuration experiments from codec-d/cool_fox Play-VIctory.cmd.
# Source: https://github.com/codec-d/cool_fox/blob/main/Play-VIctory.cmd
# Relay 0 is the baseline. Relay Next is opt-in via -RNext.
# Preserve the console profile and visibility rules.
# Test one switch at a time; neither is enabled by default.
if ($CoolFoxResidency) {
    $settings.VICTORY_CARD_BUFFERS_MIN_KB = '64'
    # Keep the existing adapter-specific allocation budget and residency guards.
}
if ($CoolFoxThin) {
    $settings.VICTORY_THIN = '1'
    $settings.VICTORY_THIN_THREADS = '8'
    # Unlike -Parallel, do not enable inline readback or change upload workers.
    $settings.VICTORY_LOG = 'warn,victory_gpu::host=info,victory_gpu::relay_next=info,victory_gpu::list::thin=info,victory_gpu::queue::behind=info'
}
$settings.VICTORY_LOG = 'warn,victory_gpu=info,victory_loader=info'
foreach ($name in $More.Keys) { $settings[$name] = $More[$name] }

$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
$reset = @(Get-ChildItem Env:VICTORY_* | Where-Object Name -NotIn $paths)
$before = @{}
Push-Location -LiteralPath $root
try {
    foreach ($name in (@($reset | Select-Object -ExpandProperty Name) + @($settings.Keys))) {
        if ($before.ContainsKey($name)) { continue }
        $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, $settings[$name], 'Process')
    }
    Start-Process -FilePath $victory `
        -ArgumentList "run `"$game`" --execute --console series-s" `
        -NoNewWindow -Wait `
        -RedirectStandardOutput "$root\victory.log" `
        -RedirectStandardError "$root\victory-errors.log"
} finally {
    foreach ($name in $before.Keys) {
        [Environment]::SetEnvironmentVariable($name, $before[$name], 'Process')
    }
    Pop-Location
}
