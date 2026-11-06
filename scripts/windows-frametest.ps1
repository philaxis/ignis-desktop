# Manual end-to-end check for Windows: does the frameless window behave like a
# window? It serves a small page that has Obsidian's tab header structure,
# starts Ignis Desktop against it and drives the mouse:
#
#   * no native title bar
#   * dragging an empty part of the tab header moves the window
#   * double-clicking it maximizes and restores
#   * the tab and the sidebar toggle still receive their clicks
#   * the minimize, maximize and close buttons work
#   * dragging to a screen edge snaps the window (Aero Snap)
#   * hovering the maximize button (Snap Layouts flyout, Windows 11) is captured
#   * with the server down, the strip along the top still moves and closes
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows-frametest.ps1 [-Exe path\to\ignis-desktop.exe]
#
# Results go to .\frametest-out\ (report.txt plus screenshots). Needs an
# interactive desktop.

param(
    [string]$Exe = $(if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release\ignis-desktop.exe' } else { 'src-tauri\target\release\ignis-desktop.exe' })
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
'@
[Win32]::SetProcessDPIAware() | Out-Null

if (-not (Test-Path $Exe)) { throw "Executable not found: $Exe" }
$out = Join-Path (Get-Location) 'frametest-out'
New-Item -ItemType Directory -Force $out | Out-Null
$port = 18081
$requests = Join-Path $out 'requests.log'
Remove-Item $requests -ErrorAction SilentlyContinue

# --- test server (background job) -------------------------------------------
# The page mimics the parts of Obsidian's DOM and CSS that the window frame
# relies on, and reports where its elements are (device pixels, relative to the
# window) and which of them received a mouse press.
$page = @'
<!doctype html><meta charset="utf-8"><title>frametest</title>
<style>
  body { margin: 0; font: 14px sans-serif; background: #fff; --frame-right-space: 0px; }
  .titlebar { position: fixed; top: 0; left: 0; right: 0; height: 39px; z-index: 30; pointer-events: none; }
  .titlebar-inner { width: 100%; display: flex; }
  .titlebar-button-container { display: flex; position: absolute; top: 0; height: 100%; pointer-events: auto; right: 0; background: #f6f6f6; }
  .workspace-tab-header-container { display: flex; height: 40px; background: #f6f6f6; border-bottom: 1px solid #ccc; align-items: stretch; padding-left: 8px; padding-right: calc(8px + var(--frame-right-space)); }
  .workspace-tab-header { width: 140px; padding: 10px; background: #fff; }
  .workspace-tab-header-new-tab { width: 24px; text-align: center; padding-top: 10px; }
  .workspace-tab-header-spacer { flex: 1; }
  .sidebar-toggle-button { width: 36px; text-align: center; padding-top: 10px; }
  #log { padding: 20px; }
</style>
<body class="is-frameless is-hidden-frameless mod-windows">
<div class="titlebar"><div class="titlebar-inner"><div class="titlebar-button-container mod-right"></div></div></div>
<div class="workspace-tabs mod-top mod-top-right-space">
  <div class="workspace-tab-header-container">
    <div class="workspace-tab-header-container-inner"><div class="workspace-tab-header">Note</div></div>
    <div class="workspace-tab-header-new-tab">+</div>
    <div class="workspace-tab-header-spacer"></div>
    <div class="sidebar-toggle-button mod-right">[]</div>
  </div>
</div>
<div id="log">Frame test page</div>
<script>
const send = (path) => fetch(path, { keepalive: true });
for (const [name, selector] of [['tab', '.workspace-tab-header'], ['toggle', '.sidebar-toggle-button']]) {
  document.querySelector(selector).addEventListener('mousedown', () => send('/__press?' + name));
}
const rect = (selector) => {
  const e = document.querySelector(selector);
  if (!e) return '';
  const b = e.getBoundingClientRect();
  return [b.x, b.y, b.width, b.height].map((v) => Math.round(v * devicePixelRatio)).join(',');
};
setInterval(() => {
  send('/__rects?dpr=' + devicePixelRatio +
    '&spacer=' + rect('.workspace-tab-header-spacer') +
    '&tab=' + rect('.workspace-tab-header') +
    '&toggle=' + rect('.sidebar-toggle-button') +
    '&min=' + rect('.ignis-frame-button.mod-minimize') +
    '&max=' + rect('.ignis-frame-button.mod-maximize') +
    '&close=' + rect('.ignis-frame-button.mod-close') +
    '&hover=' + (document.querySelector('.ignis-frame-button.mod-maximize.is-hover') ? 1 : 0));
}, 700);
</script>
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
            $body = if ($path -like '/__*' -or $path -like '/favicon.ico*') { '' } else { $page }
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

# --- throw-away config ------------------------------------------------------
$configDir = Join-Path $env:APPDATA 'dev.philaxis.ignis-desktop'
$configFile = Join-Path $configDir 'config.json'
$backup = "$configFile.frametest-backup"
New-Item -ItemType Directory -Force $configDir | Out-Null
if (Test-Path $configFile) { Move-Item -Force $configFile $backup }
$json = '{"server_url":"http://127.0.0.1:' + $port + '/","vault":"test","windows":[]}'
[IO.File]::WriteAllText($configFile, $json, (New-Object Text.UTF8Encoding $false))

$report = New-Object System.Collections.Generic.List[string]
function Say($text) { $report.Add($text); Write-Host $text }
function Shot($name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    [Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $out "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
}
# The window handle is remembered: a minimized window stops being the
# process's "main window" as far as Get-Process is concerned.
$script:Hwnd = [IntPtr]::Zero
function App-Handle {
    if ($script:Hwnd -ne [IntPtr]::Zero -and [Win32]::IsWindow($script:Hwnd)) { return $script:Hwnd }
    $p = Get-Process ignis-desktop -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    $script:Hwnd = if ($p) { $p.MainWindowHandle } else { [IntPtr]::Zero }
    $script:Hwnd
}
function Get-Rect {
    $h = App-Handle
    if ($h -eq [IntPtr]::Zero) { return $null }
    $r = New-Object Win32+RECT
    [Win32]::GetWindowRect($h, [ref]$r) | Out-Null
    $r
}
function Describe($r) { if ($r) { "L=$($r.Left) T=$($r.Top) W=$($r.Right - $r.Left) H=$($r.Bottom - $r.Top)" } else { 'no window' } }
function Focus-App {
    $h = App-Handle
    if ($h -ne [IntPtr]::Zero) {
        if ([Win32]::IsIconic($h)) { [Win32]::ShowWindow($h, 9) | Out-Null }
        [Win32]::SetForegroundWindow($h) | Out-Null
    }
    Start-Sleep -Milliseconds 400
}
function Mouse-To($x, $y) { [Win32]::SetCursorPos([int]$x, [int]$y) | Out-Null }
function Mouse-Down { [Win32]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero) }
function Mouse-Up { [Win32]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero) }
function Click($x, $y) { Mouse-To $x $y; Start-Sleep -Milliseconds 150; Mouse-Down; Mouse-Up; Start-Sleep -Milliseconds 600 }
function Double-Click($x, $y) {
    Mouse-To $x $y; Start-Sleep -Milliseconds 200
    Mouse-Down; Mouse-Up; Start-Sleep -Milliseconds 90; Mouse-Down; Mouse-Up
    Start-Sleep -Milliseconds 1000
}
function Drag($x, $y, $toX, $toY) {
    Mouse-To $x $y; Start-Sleep -Milliseconds 200
    Mouse-Down; Start-Sleep -Milliseconds 500
    $steps = 25
    for ($i = 1; $i -le $steps; $i++) {
        Mouse-To ($x + ($toX - $x) * $i / $steps) ($y + ($toY - $y) * $i / $steps)
        Start-Sleep -Milliseconds 25
    }
    Start-Sleep -Milliseconds 600
    Mouse-Up
    Start-Sleep -Milliseconds 1000
}
# Latest element rectangles the page reported: name -> (x, y, w, h) in pixels
# relative to the window.
function Page-Rects {
    $line = if (Test-Path $requests) { Get-Content $requests | Where-Object { $_ -like '/__rects*' } | Select-Object -Last 1 } else { $null }
    $map = @{}
    if ($line) {
        foreach ($pair in ($line -split '\?', 2)[1] -split '&') {
            $k, $v = $pair -split '=', 2
            if ($v) { $map[$k] = @($v -split ',' | ForEach-Object { [int]$_ }) }
        }
    }
    $map
}
# Where the page's top-left corner is on screen.
function Client-Origin {
    $p = New-Object Win32+POINT
    [Win32]::ClientToScreen((App-Handle), [ref]$p) | Out-Null
    $p
}
# Centre of a page element on screen.
function Center-Of($name) {
    $rects = Page-Rects
    if (-not $rects.ContainsKey($name)) { return $null }
    $o = Client-Origin
    $e = $rects[$name]
    @(($o.X + $e[0] + $e[2] / 2), ($o.Y + $e[1] + $e[3] / 2))
}
function Presses($name) {
    if (-not (Test-Path $requests)) { return 0 }
    @(Get-Content $requests | Where-Object { $_ -eq "/__press?$name" }).Count
}
function App-Running { [bool](Get-Process ignis-desktop -ErrorAction SilentlyContinue) }
function Stop-App { Get-Process ignis-desktop -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Seconds 2 }

$screen = [Windows.Forms.SystemInformation]::WorkingArea
Say "Working area: $($screen.Width)x$($screen.Height) at $($screen.X),$($screen.Y)"
try {
    # --- server up: Obsidian-like page ------------------------------------
    Start-Process $Exe | Out-Null
    Start-Sleep -Seconds 12
    Focus-App
    Shot '00-start'
    $r = Get-Rect
    $o = Client-Origin
    Say "Window: $(Describe $r)"
    Say "Page starts $($o.Y - $r.Top) px below the top of the window (no native title bar: $(($o.Y - $r.Top) -lt 8))"
    $rects = Page-Rects
    Say "Page reported: $(($rects.Keys | Sort-Object | ForEach-Object { "$_=$($rects[$_] -join ',')" }) -join ' ')"
    Say "Window controls drawn in the tab header (min/max/close rects reported): $([bool]($rects['min'] -and $rects['max'] -and $rects['close']))"

    # Presses on the tab and the sidebar toggle must reach the page, not drag.
    $before = Get-Rect
    $c = Center-Of 'tab';    if ($c) { Click $c[0] $c[1] }
    $c = Center-Of 'toggle'; if ($c) { Click $c[0] $c[1] }
    Say "Tab received the press: $((Presses 'tab') -ge 1); sidebar toggle received it: $((Presses 'toggle') -ge 1)"
    Say "Window did not move on those clicks: $((Describe (Get-Rect)) -eq (Describe $before))"

    # Drag from the empty tab header.
    $c = Center-Of 'spacer'
    $before = Get-Rect
    Drag $c[0] $c[1] ($c[0] + 160) ($c[1] + 90)
    $after = Get-Rect
    Say "Drag by (160, 90): $(Describe $before) -> $(Describe $after); moved: $($after.Left - $before.Left),$($after.Top - $before.Top)"
    Shot '01-dragged'

    # Double click maximizes, again restores.
    $c = Center-Of 'spacer'
    Double-Click $c[0] $c[1]
    Say "Double click maximizes: $([Win32]::IsZoomed((App-Handle))) ($(Describe (Get-Rect)))"
    Shot '02-maximized'
    $c = Center-Of 'spacer'
    Double-Click $c[0] $c[1]
    Say "Second double click restores: $(-not [Win32]::IsZoomed((App-Handle))) ($(Describe (Get-Rect)))"

    # Edge snapping: left edge, then right edge, then top edge (maximize).
    foreach ($edge in 'left', 'right', 'top') {
        Focus-App
        $c = Center-Of 'spacer'
        switch ($edge) {
            'left'  { $tx = $screen.Left + 1;  $ty = $screen.Top + 300 }
            'right' { $tx = $screen.Right - 2; $ty = $screen.Top + 300 }
            'top'   { $tx = $screen.Left + $screen.Width / 2; $ty = $screen.Top }
        }
        Drag $c[0] $c[1] $tx $ty
        $r = Get-Rect
        Say "Snap to the $edge edge: $(Describe $r); zoomed: $([Win32]::IsZoomed((App-Handle)))"
        Shot "03-snap-$edge"
        if ([Win32]::IsZoomed((App-Handle))) {
            $c = Center-Of 'spacer'
            Double-Click $c[0] $c[1]
        } else {
            # Pull it back out of the snapped state.
            $c = Center-Of 'spacer'
            Drag $c[0] $c[1] ($screen.Left + 900) ($screen.Top + 200)
        }
    }

    # Snap Layouts: hover the maximize button and wait for the flyout. The
    # native window over the button reports the hover back to the page.
    Focus-App
    $c = Center-Of 'max'
    Mouse-To $c[0] $c[1]; Start-Sleep -Milliseconds 300
    Mouse-To ($c[0] + 1) $c[1]
    Start-Sleep -Seconds 2
    Shot '04-hover-maximize'
    Say "Hovering the maximize button reached the page: $((Page-Rects)['hover'] -eq 1)"

    # Buttons.
    Focus-App
    $c = Center-Of 'min'; Click $c[0] $c[1]
    Say "Minimize button minimizes: $([Win32]::IsIconic((App-Handle)))"
    Shot '05-minimized'
    [Win32]::ShowWindow((App-Handle), 9) | Out-Null
    Start-Sleep -Seconds 1
    Focus-App
    $c = Center-Of 'max'; Click $c[0] $c[1]
    Say "Maximize button maximizes: $([Win32]::IsZoomed((App-Handle)))"
    Shot '06-maximize-button'
    $c = Center-Of 'max'; Click $c[0] $c[1]
    Say "Maximize button restores: $(-not [Win32]::IsZoomed((App-Handle)))"
    $c = Center-Of 'close'; Click $c[0] $c[1]
    Start-Sleep -Seconds 2
    Say "Close button closes the (only) window and quits: $(-not (App-Running))"
    Stop-App

    # --- server down: the strip along the top ------------------------------
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
    Start-Process $Exe | Out-Null
    Start-Sleep -Seconds 10
    Focus-App
    Shot '07-offline'
    $r = Get-Rect
    Say "Offline window: $(Describe $r)"
    $before = $r
    Drag ($r.Left + ($r.Right - $r.Left) / 2) ($r.Top + 12) ($r.Left + ($r.Right - $r.Left) / 2 + 120) ($r.Top + 80)
    $after = Get-Rect
    Say "Offline drag by the strip moved: $($after.Left - $before.Left),$($after.Top - $before.Top)"
    Shot '08-offline-dragged'
    $r = Get-Rect
    Click ($r.Right - 20) ($r.Top + 14)
    Start-Sleep -Seconds 2
    Say "Offline close button quits: $(-not (App-Running))"
} finally {
    Stop-App
    Stop-Job $server -ErrorAction SilentlyContinue
    Remove-Job $server -Force -ErrorAction SilentlyContinue
    Remove-Item $configFile -ErrorAction SilentlyContinue
    if (Test-Path $backup) { Move-Item -Force $backup $configFile }
    $report | Set-Content -Encoding UTF8 (Join-Path $out 'report.txt')
}
