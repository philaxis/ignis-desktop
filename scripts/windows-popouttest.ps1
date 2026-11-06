# Manual end-to-end check for Windows: do Obsidian pop-out windows work in
# Ignis Desktop?
#
# It points a throw-away Ignis Desktop config at a real Ignis server, starts the
# app with the WebView2 debugging port open, and then:
#   1. creates a test note, shows only that note in the window,
#   2. moves it into a pop-out (Obsidian command, run over the debugging port),
#   3. types into the pop-out with real key presses,
#   4. closes the pop-out window and checks that its tab is gone,
#   5. pops out again, drags the tab back into the main window with the mouse,
#   6. opens one more pop-out, closes the main window and checks that the
#      pop-out went with it,
# taking a screenshot after each step.
#
# The server needs a vault that may get a throw-away note and a throw-away
# workspace layout (`workspace.popouttest.json`). The note is deleted at the end;
# delete the layout file yourself.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows-popouttest.ps1 `
#       -Server https://ignis.example.ts.net -Vault vault [-Exe path\to\ignis-desktop.exe]
#
# Results go to .\popouttest-out\ (report.txt plus screenshots).

param(
    [Parameter(Mandatory = $true)][string]$Server,
    [string]$Vault = 'vault',
    [string]$Exe = $(if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'release\ignis-desktop.exe' } else { 'src-tauri\target\release\ignis-desktop.exe' }),
    [int]$DebugPort = 9333
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class Win32 {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    // Visible top-level windows of any process whose title contains a phrase.
    public static List<IntPtr> WindowsTitled(string phrase) {
        var found = new List<IntPtr>();
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var s = new StringBuilder(256); GetWindowText(h, s, 256);
            if (s.ToString().Contains(phrase)) found.Add(h);
            return true;
        }, IntPtr.Zero);
        return found;
    }
    // Visible top-level windows of a process that have a title and a real size.
    public static List<IntPtr> WindowsOf(uint pid) {
        var found = new List<IntPtr>();
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p != pid || !IsWindowVisible(h)) return true;
            var s = new StringBuilder(256); GetWindowText(h, s, 256);
            RECT r; GetWindowRect(h, out r);
            if (s.Length > 0 && r.Right - r.Left > 150 && r.Bottom - r.Top > 100) found.Add(h);
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@
[Win32]::SetProcessDPIAware() | Out-Null

if (-not (Test-Path $Exe)) { throw "Executable not found: $Exe" }
# An earlier run that sent Alt+F4 to the desktop may have left the Windows
# shut-down dialog open. Close it without confirming.
foreach ($phrase in 'Windows 종료', 'Shut Down Windows') {
    foreach ($h in [Win32]::WindowsTitled($phrase)) { [Win32]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
}
$out = Join-Path (Get-Location) 'popouttest-out'
New-Item -ItemType Directory -Force $out | Out-Null
$testNote = 'zz-popout-test.md'
$windowId = 'popouttest'

# --- throw-away config ------------------------------------------------------
$configDir = Join-Path $env:APPDATA 'dev.philaxis.ignis-desktop'
$configFile = Join-Path $configDir 'config.json'
$backup = "$configFile.popouttest-backup"
New-Item -ItemType Directory -Force $configDir | Out-Null
if (Test-Path $configFile) { Move-Item -Force $configFile $backup }
$json = (@{
    server_url = $Server
    vault      = $Vault
    windows    = @(@{ id = $windowId; geometry = @{ x = 60; y = 40; width = 1000; height = 650; maximized = $false } })
} | ConvertTo-Json -Depth 5 -Compress)
[IO.File]::WriteAllText($configFile, $json, (New-Object Text.UTF8Encoding $false))

$report = New-Object System.Collections.Generic.List[string]
function Note($text) { $report.Add($text); Write-Host $text }
function Check($label, $ok) { Note ("{0,-70} {1}" -f $label, $(if ($ok) { 'PASS' } else { 'FAIL' })) }

function Shot($name) {
    $b = [Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    [Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $out "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
}

# --- DevTools protocol over the WebView2 debugging port ----------------------
# Windows PowerShell hands the JSON array over as one object; @() unwraps it.
function Get-Targets { $all = Invoke-RestMethod "http://127.0.0.1:$DebugPort/json"; @(@($all) | Where-Object { $_.type -eq 'page' }) }
function Get-MainTarget { Get-Targets | Where-Object { $_.url -like "http*workspace=$windowId*" } | Select-Object -First 1 }
function Get-PopoutTargets { @(Get-Targets | Where-Object { $_.url -eq 'about:blank' }) }

function Invoke-Cdp($target, $expression) {
    $ws = New-Object Net.WebSockets.ClientWebSocket
    $none = [Threading.CancellationToken]::None
    $ws.ConnectAsync([Uri]$target.webSocketDebuggerUrl, $none).Wait()
    $message = @{
        id = 1; method = 'Runtime.evaluate'
        params = @{ expression = $expression; awaitPromise = $true; returnByValue = $true; userGesture = $true }
    } | ConvertTo-Json -Depth 5 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($message)
    $ws.SendAsync([ArraySegment[byte]]::new($bytes), 'Text', $true, $none).Wait()
    $buffer = New-Object byte[] 65536
    $reply = $null
    while (-not $reply) {
        $text = ''
        do {
            $chunk = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), $none).Result
            $text += [Text.Encoding]::UTF8.GetString($buffer, 0, $chunk.Count)
        } until ($chunk.EndOfMessage)
        $parsed = $text | ConvertFrom-Json
        if ($parsed.id -eq 1) { $reply = $parsed }
    }
    $ws.Dispose()
    if ($reply.result.exceptionDetails) { throw ("Page error: " + ($reply.result.exceptionDetails | ConvertTo-Json -Depth 4 -Compress)) }
    $reply.result.result.value
}
function Eval-Main($expression) { Invoke-Cdp (Get-MainTarget) $expression }

# Number of app windows and of about:blank pages right now.
function Popout-State { [pscustomobject]@{ Windows = @(App-Windows).Count; Pages = @(Get-PopoutTargets).Count } }
function Wait-State($seconds, [scriptblock]$test) {
    for ($i = 0; $i -lt ($seconds * 2); $i++) {
        $state = Popout-State
        if (& $test $state) { return $true }
        Start-Sleep -Milliseconds 500
    }
    $false
}
function Wait-Until($seconds, [scriptblock]$condition) {
    $deadline = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $deadline) {
        try { if (& $condition) { return $true } } catch { $script:lastWaitError = $_.Exception.Message }
        Start-Sleep -Milliseconds 500
    }
    $false
}

# --- window helpers ---------------------------------------------------------
function App-Pid { (Get-Process ignis-desktop -ErrorAction SilentlyContinue | Select-Object -First 1).Id }
function App-Windows { $p = App-Pid; if ($p) { @([Win32]::WindowsOf([uint32]$p)) } else { @() } }
function Window-Title($h) { $s = New-Object Text.StringBuilder 256; [Win32]::GetWindowText($h, $s, 256) | Out-Null; $s.ToString() }
function Window-Rect($h) { $r = New-Object Win32+RECT; [Win32]::GetWindowRect($h, [ref]$r) | Out-Null; $r }
# Same as the window's close button. Never Alt+F4: with no app window in front
# it would go to the desktop and open the Windows shut-down dialog.
function Close-Window($h) { [Win32]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
function Focus($h) { [Win32]::ShowWindow($h, 9) | Out-Null; [Win32]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 600 }
function Click-At($x, $y) {
    [Win32]::SetCursorPos([int]$x, [int]$y) | Out-Null
    [Win32]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
    [Win32]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 400
}
# Screen position (physical pixels) of the centre of a page element.
function Element-Center($target, $selector) {
    $js = "(() => { const e = document.querySelector('$selector'); if (!e) return null; const r = e.getBoundingClientRect(); const d = window.devicePixelRatio;" +
          " return JSON.stringify({ x: Math.round((window.screenX + r.x + r.width / 2) * d), y: Math.round((window.screenY + (window.outerHeight - window.innerHeight) + r.y + r.height / 2) * d) }); })()"
    $v = Invoke-Cdp $target $js
    if ($v) { $v | ConvertFrom-Json }
}

$cmdPopout = "app.commands.executeCommandById('workspace:move-to-new-window')"
$showOnlyTestNote = @"
(async () => {
  const path = '$testNote';
  const old = app.vault.getAbstractFileByPath(path);
  if (old) await app.vault.delete(old);
  const file = await app.vault.create(path, '# Pop-out test\n\nThis note was opened in the main window.\n');
  const leaf = app.workspace.getMostRecentLeaf(app.workspace.rootSplit) || app.workspace.getLeaf('tab');
  await leaf.openFile(file, { state: { mode: 'source' } });
  app.workspace.iterateRootLeaves((other) => { if (other !== leaf) other.detach(); });
  app.workspace.leftSplit.collapse(); app.workspace.rightSplit.collapse();
  app.workspace.setActiveLeaf(leaf, { focus: true });
  return true;
})()
"@
$noteContainer = "(() => { const l = app.workspace.getLeavesOfType('markdown').filter(l => l.view.file && l.view.file.path === '$testNote'); return l.map(l => l.getContainer().type).join(',') })()"

$app = $null
try {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$DebugPort"
    $app = Start-Process $Exe -PassThru
    Note "Server: (not recorded)  Vault: $Vault  Exe: $Exe"

    try { $reach = (Invoke-WebRequest -UseBasicParsing $Server -TimeoutSec 20).StatusCode } catch { $reach = $_.Exception.Message }
    Note "Server reachable from this machine: $reach"
    # A data folder that has not seen the vault asks whether to trust it, and
    # the layout stays unready until that is answered. Keep restricted mode.
    $trustButton = "(() => { const b = document.querySelector('.modal-container .modal-button-container button:not(.mod-cta)'); if (b) b.click(); return !!(window.app && app.workspace && app.workspace.layoutReady) })()"
    $up = Wait-Until 90 { (Get-MainTarget) -and (Eval-Main $trustButton) }
    Check 'Main window loaded Ignis' $up
    if (-not $up) {
        Shot '00-failed-to-load'
        try { Note ("Page answered: " + (Eval-Main 'document.title')) } catch { Note "Page did not answer: $($_ | Out-String)" }
        try { Note ("Debugging port targets: " + ((Get-Targets | ForEach-Object { $_.url -replace '^https?://[^/]+', '<server>' }) -join ' | ')) } catch { Note "Debugging port not reachable: $($_.Exception.Message)" }
        throw 'Ignis did not load in the window.'
    }

    Start-Sleep -Seconds 2
    Eval-Main $showOnlyTestNote | Out-Null
    Start-Sleep -Seconds 2
    $mainWindow = (App-Windows)[0]
    Focus $mainWindow
    Shot '01-main-window'

    # 1. Pop out
    $before = (App-Windows).Count
    Eval-Main $cmdPopout | Out-Null
    $popped = Wait-State 15 { param($st) $st.Windows -gt $before -and $st.Pages -ge 1 }
    Check 'A pop-out window opened (window count +1, one about:blank page)' $popped
    if (-not $popped) { Note "  last error while waiting: $script:lastWaitError" }
    Note ("  windows before/after: $before/$((App-Windows).Count); pages: " + ((Get-Targets | ForEach-Object { $_.url -replace '^https?://[^/]+', '<server>' }) -join ' | '))
    Note ("  window titles: " + ((App-Windows | ForEach-Object { Window-Title $_ }) -join ' | '))
    Start-Sleep -Seconds 2
    $popout = $null
    if ($popped) {
        $popout = (App-Windows) | Where-Object { $_ -ne $mainWindow } | Select-Object -First 1
        Check 'Pop-out window title is the note title (Obsidian set it)' ($popout -and (Window-Title $popout) -like '*zz-popout-test*')
        Check 'Note now lives in a window container, not in the main window' ((Eval-Main $noteContainer) -eq 'window')
        $popoutPage = (Get-PopoutTargets)[0]
        $opener = Invoke-Cdp $popoutPage '!!window.opener'
        Check 'Pop-out keeps window.opener' ($opener -eq $true)
        $r = Window-Rect $popout
        Note ("Pop-out size {0}x{1} at {2},{3}" -f ($r.Right - $r.Left), ($r.Bottom - $r.Top), $r.Left, $r.Top)
    }
    if ($popout) {
        Focus $popout
        Shot '02-popout'

        # 2. Type into the pop-out
        $r = Window-Rect $popout
        Click-At (($r.Left + $r.Right) / 2) ($r.Top + 300)
        [Windows.Forms.SendKeys]::SendWait('^{END}')
        [Windows.Forms.SendKeys]::SendWait('typed in the pop-out')
        Start-Sleep -Seconds 3
        $text = Eval-Main "app.vault.adapter.read('$testNote')"
        Check 'Typing in the pop-out reached the note on the server' ($text -like '*typed in the pop-out*')
        Shot '03-typed-in-popout'

        # 3. Close the window the way its close button does
        Close-Window $popout
        $closed = Wait-State 10 { param($st) $st.Windows -le $before -and $st.Pages -eq 0 }
        Check 'Closing the pop-out window removed it' $closed
        Start-Sleep -Seconds 2
        Check 'Its tab was removed from the layout (floating window gone)' ((Eval-Main 'app.workspace.floatingSplit.children.length') -eq 0)
        Shot '04-popout-closed'
    }

    function Test-DragBack($label, [scriptblock]$prepare) {
    foreach ($w in @(App-Windows | Where-Object { $_ -ne $mainWindow })) { Close-Window $w }
    Start-Sleep -Seconds 3
    & $prepare
    Eval-Main $showOnlyTestNote | Out-Null
    Start-Sleep -Seconds 1
    Eval-Main $cmdPopout | Out-Null
    $popped = Wait-State 15 { param($st) $st.Pages -ge 1 }
    Start-Sleep -Seconds 2
    if ($popped) {
        $popTarget = (Get-PopoutTargets)[0]
        $from = Element-Center $popTarget '.workspace-tab-header'
        $mainTarget = Get-MainTarget
        $to = Element-Center $mainTarget '.workspace-tab-header-container-inner'
        Note "Drag from $($from.x),$($from.y) to $($to.x),$($to.y)"
        $popWin = (App-Windows) | Where-Object { $_ -ne $mainWindow } | Select-Object -First 1
        Focus $popWin
        [Win32]::SetCursorPos($from.x, $from.y) | Out-Null
        Start-Sleep -Milliseconds 300
        [Win32]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
        for ($i = 1; $i -le 30; $i++) {
            $x = [int]($from.x + ($to.x - $from.x) * $i / 30)
            $y = [int]($from.y + ($to.y - $from.y) * $i / 30)
            [Win32]::SetCursorPos($x, $y) | Out-Null
            Start-Sleep -Milliseconds 40
        }
        Start-Sleep -Milliseconds 400
        [Win32]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
        $merged = Wait-Until 10 { (Eval-Main $noteContainer) -eq 'split' }
        Check "Dragging the tab back put it into the main window again ($label)" $merged
        Start-Sleep -Seconds 2
        Check "The now-empty pop-out closed itself ($label)" (Wait-State 8 { param($st) $st.Pages -eq 0 -and $st.Windows -le $before })
        Shot "05-merged-back-$label"
    } else {
        Check "Second pop-out opened ($label)" $false
    }
    }

    # 4. Pop out again and drag the tab back, with and without the frame script in the pop-out
    Test-DragBack 'frame in pop-out' { }
    Test-DragBack 'no frame script in pop-out' { Eval-Main 'delete window.__ignisFrameSource' | Out-Null }

    # 5. Closing the main window closes its pop-outs
    Eval-Main "app.vault.getAbstractFileByPath('$testNote') && app.vault.delete(app.vault.getAbstractFileByPath('$testNote')).then(() => true)" | Out-Null
    Eval-Main "(() => { app.workspace.openPopoutLeaf(); return true })()" | Out-Null
    Start-Sleep -Seconds 3
    $withPopout = (App-Windows).Count
    Check 'A second window is open before closing the main window' ($withPopout -ge 2)
    Shot '06-before-closing-main'
    Close-Window $mainWindow
    $gone = Wait-Until 15 { -not (Get-Process ignis-desktop -ErrorAction SilentlyContinue) }
    Check 'Closing the main window closed the pop-out and ended the app' $gone
    Shot '07-after-closing-main'
} catch {
    Note "ERROR: $($_.Exception.Message)"
} finally {
    Get-Process ignis-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item Env:\WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
    Remove-Item $configFile -ErrorAction SilentlyContinue
    if (Test-Path $backup) { Move-Item -Force $backup $configFile }
    $report | Set-Content -Encoding UTF8 (Join-Path $out 'report.txt')
}
