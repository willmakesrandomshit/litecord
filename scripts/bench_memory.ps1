param(
    [string]$ExePath = "C:\Users\Owner\.gemini\antigravity\scratch\discord-zero\target\release\discord-zero.exe"
)

$binMB = [math]::Round((Get-Item $ExePath).Length / 1MB, 2)
Write-Output "Binary Size: ${binMB} MB"

function Measure-Run {
    param(
        [string]$Label,
        [string]$Backend,
        [string[]]$Args,
        [int[]]$SampleSeconds
    )
    $env:ICED_BACKEND = $Backend
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    if ($Args.Count -gt 0) {
        $p = Start-Process -FilePath $ExePath -ArgumentList $Args -PassThru
    } else {
        $p = Start-Process -FilePath $ExePath -PassThru
    }

    # Measure startup latency until MainWindowHandle appears (up to 5s)
    $windowReadyMs = -1
    while ($sw.ElapsedMilliseconds -lt 5000) {
        $p.Refresh()
        if ($p.HasExited) { break }
        if ($p.MainWindowHandle -ne [IntPtr]::Zero) {
            $windowReadyMs = $sw.ElapsedMilliseconds
            break
        }
        Start-Sleep -Milliseconds 20
    }

    $elapsed = 0
    $samples = @()
    foreach ($t in $SampleSeconds) {
        $wait = $t - $elapsed
        if ($wait -gt 0) { Start-Sleep -Seconds $wait }
        $elapsed = $t
        $p.Refresh()
        if ($p.HasExited) {
            $samples += "${t}s:EXITED($($p.ExitCode))"
        } else {
            $ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
            $priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
            $cpu = [math]::Round($p.TotalProcessorTime.TotalSeconds, 3)
            $samples += "${t}s:WS=${ws}MB/PRIV=${priv}MB/CPU=${cpu}s"
        }
    }

    $p.Refresh()
    $alive = -not $p.HasExited
    if ($alive) {
        $p.CloseMainWindow() | Out-Null
        Start-Sleep -Milliseconds 400
        if (-not $p.HasExited) { $p.Kill() }
    }
    Remove-Item Env:ICED_BACKEND -ErrorAction SilentlyContinue
    Write-Output "$Label | backend=$Backend | startup=${windowReadyMs}ms | alive=$alive | $($samples -join ' | ')"
}

Measure-Run -Label "idle_wgpu" -Backend "wgpu" -Args @() -SampleSeconds @(3, 10, 30)
Measure-Run -Label "idle_tiny_skia" -Backend "tiny-skia" -Args @() -SampleSeconds @(3, 10, 30, 60)
Measure-Run -Label "synth_100" -Backend "tiny-skia" -Args @("--synthetic", "100") -SampleSeconds @(3)
Measure-Run -Label "synth_1k" -Backend "tiny-skia" -Args @("--synthetic", "1000") -SampleSeconds @(3)
Measure-Run -Label "synth_10k" -Backend "tiny-skia" -Args @("--synthetic", "10000") -SampleSeconds @(3)
Measure-Run -Label "synth_100k" -Backend "tiny-skia" -Args @("--synthetic", "100000") -SampleSeconds @(3)
Measure-Run -Label "synth_100k_wgpu" -Backend "wgpu" -Args @("--synthetic", "100000") -SampleSeconds @(3)
