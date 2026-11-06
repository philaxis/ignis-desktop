# Manual end-to-end check for Windows: does every browser shortcut reach the
# page instead of being handled by WebView2?
#
# It serves a tiny local test page, points a throw-away Ignis Desktop config at
# it, starts the app, sends real key presses (SendKeys) and records what the
# page sees and what the server was asked for. Needs an interactive desktop.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows-keytest.ps1 [-Exe path\to\ignis-desktop.exe]
#
# Results go to .\keytest-out\ (report.txt plus screenshots).

param(
    [string]$Exe = $(if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release\ignis-desktop.exe' } else { 'src-tauri\target\release\ignis-desktop.exe' })
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@

if (-not (Test-Path $Exe)) { throw "Executable not found: $Exe" }
$out = Join-Path (Get-Location) 'keytest-out'
New-Item -ItemType Directory -Force $out | Out-Null
$port = 18080
$requests = Join-Path $out 'requests.log'
Remove-Item $requests -ErrorAction SilentlyContinue

# --- test server (background job) -------------------------------------------
$page = @'
<!doctype html><meta charset="utf-8"><title>keytest</title>
<body style="font:20px sans-serif"><p>Ignis Desktop key test page</p><pre id="log"></pre>
<script>
addEventListener('keydown', (e) => {
  const k = (e.ctrlKey ? 'Ctrl+' : '') + (e.shiftKey ? 'Shift+' : '') + (e.altKey ? 'Alt+' : '') + e.key;
  document.getElementById('log').textContent += k + '\n';
  fetch('/__key?k=' + encodeURIComponent(k), { keepalive: true });
});
</script></body>
'@
$server = Start-Job -ArgumentList $port, $page, $requests -ScriptBlock {
    param($port, $page, $requests)
    try {
    $listener = New-Object System.Net.Sockets.TcpListener([Net.IPAddress]::Loopback, [int]$port)
    $listener.Start()
    while ($true) {
        $client = $listener.AcceptTcpClient()
        try {
            $stream = $client.GetStream()
            $buffer = New-Object byte[] 8192
            $n = $stream.Read($buffer, 0, $buffer.Length)
            $head = [Text.Encoding]::ASCII.GetString($buffer, 0, $n)
            $path = ($head -split ' ')[1]
            Add-Content -Path $requests -Value $path
            $body = if ($path -like '/__key*') { '' } else { $page }
            $bytes = [Text.Encoding]::UTF8.GetBytes($body)
            $header = "HTTP/1.1 200 OK`r`nContent-Type: text/html; charset=utf-8`r`nContent-Length: $($bytes.Length)`r`nConnection: close`r`n`r`n"
            $h = [Text.Encoding]::ASCII.GetBytes($header)
            $stream.Write($h, 0, $h.Length)
            $stream.Write($bytes, 0, $bytes.Length)
        } catch { }
        $client.Close()
    }
    } catch { $_ | Out-String | Set-Content "$requests.err" }
}
Start-Sleep -Seconds 3
try { $self = (Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/__selfcheck").StatusCode } catch { $self = $_.Exception.Message }
Remove-Item $requests -ErrorAction SilentlyContinue

# --- throw-away config ------------------------------------------------------
$configDir = Join-Path $env:APPDATA 'dev.philaxis.ignis-desktop'
$configFile = Join-Path $configDir 'config.json'
$backup = "$configFile.keytest-backup"
New-Item -ItemType Directory -Force $configDir | Out-Null
if (Test-Path $configFile) { Move-Item -Force $configFile $backup }
$json = '{"server_url":"http://127.0.0.1:' + $port + '/","vault":"test","windows":[]}'
[IO.File]::WriteAllText($configFile, $json, (New-Object Text.UTF8Encoding $false))

$report = New-Object System.Collections.Generic.List[string]
function Shot($name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    [Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $out "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
}
function Focus-App {
    $p = Get-Process ignis-desktop -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if ($p) { [Win32]::ShowWindow($p.MainWindowHandle, 9) | Out-Null; [Win32]::SetForegroundWindow($p.MainWindowHandle) | Out-Null }
    Start-Sleep -Milliseconds 500
}
# Real key presses only reach the page once the web content has keyboard focus,
# so click into it first.
function Click-Page {
    $p = Get-Process ignis-desktop -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $p) { return }
    $r = New-Object Win32+RECT
    [Win32]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
    [Win32]::SetCursorPos([int](($r.Left + $r.Right) / 2), [int]($r.Top + 300)) | Out-Null
    [Win32]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
    [Win32]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500
}
function Send($keys, $label) {
    Focus-App
    Click-Page
    [Windows.Forms.SendKeys]::SendWait($keys)
    Start-Sleep -Milliseconds 1500
    Shot $label
}

$app = $null
try {
    $app = Start-Process $Exe -PassThru
    Start-Sleep -Seconds 10
    Shot '00-start'
    $loads = if (Test-Path $requests) { @(Get-Content $requests | Where-Object { $_ -notlike '/__key*' -and $_ -ne '/favicon.ico' }).Count } else { 0 }

    Send '^p' '01-ctrl-p'
    Send '^f' '02-ctrl-f'
    Send '{F5}' '03-f5'
    Send '^r' '04-ctrl-r'
    Send '^u' '05-ctrl-u'
    Send '^+n' '06-ctrl-shift-n'
    Start-Sleep -Seconds 5
    Shot '07-final'
} finally {
    Get-Process ignis-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Remove-Item $configFile -ErrorAction SilentlyContinue
    if (Test-Path $backup) { Move-Item -Force $backup $configFile }
}

$log = if (Test-Path $requests) { @(Get-Content $requests) } else { @() }
$keys = @($log | Where-Object { $_ -like '/__key*' } | ForEach-Object { [Uri]::UnescapeDataString(($_ -split 'k=')[1]) })
$pageLoads = @($log | Where-Object { $_ -notlike '/__key*' -and $_ -ne '/favicon.ico' })
$report.Add("Test server self-check: $self")
if (Test-Path "$requests.err") { $report.Add("Server error: $(Get-Content "$requests.err" -Raw)") }
$report.Add("Keys seen by the page: $($keys -join ', ')")
$report.Add("Page loads: $($pageLoads.Count) (before the key presses: $loads)")
$pageLoads | ForEach-Object { $report.Add("  GET $_") }
foreach ($k in 'Ctrl+p', 'Ctrl+f', 'F5', 'Ctrl+r', 'Ctrl+u') {
    $report.Add("page received $k : $($keys -contains $k)")
}
$mainLoads = @($pageLoads | Where-Object { $_ -notlike '*workspace*' }).Count
$report.Add("main window loaded once (no reload after F5 / Ctrl+R): $($mainLoads -eq 1)")
$report.Add("Ctrl+Shift+N opened a workspace window: $([bool]($pageLoads | Where-Object { $_ -like '*workspace=w2*' }))")
$report | Set-Content -Encoding UTF8 (Join-Path $out 'report.txt')
$report
