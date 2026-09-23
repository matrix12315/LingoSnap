# Capture Selection Translate popup / manager windows for UI redesign QA.
# Writes PNG screenshots under windows/tmp/ui-redesign-qa/<stamp>/.
# Dev-only tool; product code must not retain screenshots.

param(
    [string]$OutRoot = "",
    [string]$ProcessName = "",
    [switch]$AllTopLevel
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Win32Cap {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

function Resolve-OutRoot {
  if ($OutRoot) { return $OutRoot }
  $repo = Split-Path -Parent $PSScriptRoot
  $stamp = Get-Date -Format "yyyyMMddHHmmss"
  return Join-Path $repo "tmp\ui-redesign-qa\$stamp"
}

function Capture-Hwnd([IntPtr]$hwnd, [string]$path) {
  $rect = New-Object Win32Cap+RECT
  if (-not [Win32Cap]::GetWindowRect($hwnd, [ref]$rect)) { return $false }
  $w = [Math]::Max(1, $rect.Right - $rect.Left)
  $h = [Math]::Max(1, $rect.Bottom - $rect.Top)
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  $ok = [Win32Cap]::PrintWindow($hwnd, $hdc, 2) # PW_RENDERFULLCONTENT
  $g.ReleaseHdc($hdc)
  $g.Dispose()
  if (-not $ok) {
    # Fallback: screen copy of the window rect.
    $g2 = [System.Drawing.Graphics]::FromImage($bmp)
    $g2.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))
    $g2.Dispose()
  }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return $true
}

$out = Resolve-OutRoot
New-Item -ItemType Directory -Force -Path $out | Out-Null
Write-Host "Output: $out"

$targets = New-Object System.Collections.Generic.List[object]
$callback = {
  param($hwnd, $lparam)
  if (-not [Win32Cap]::IsWindowVisible($hwnd)) { return $true }
  $pid2 = 0
  [void][Win32Cap]::GetWindowThreadProcessId($hwnd, [ref]$pid2)
  $proc = Get-Process -Id $pid2 -ErrorAction SilentlyContinue
  if (-not $proc) { return $true }
  if ($ProcessName -and $proc.ProcessName -notlike "*$ProcessName*") { return $true }
  if (-not $ProcessName -and $proc.ProcessName -notmatch "selection-translate|resident|manager") { return $true }
  $sb = New-Object System.Text.StringBuilder 256
  [void][Win32Cap]::GetWindowTextW($hwnd, $sb, $sb.Capacity)
  $title = $sb.ToString()
  $cb = New-Object System.Text.StringBuilder 256
  [void][Win32Cap]::GetClassNameW($hwnd, $cb, $cb.Capacity)
  $class = $cb.ToString()
  $rect = New-Object Win32Cap+RECT
  [void][Win32Cap]::GetWindowRect($hwnd, [ref]$rect)
  $targets.Add([pscustomobject]@{
    Hwnd = $hwnd
    Title = $title
    Class = $class
    Process = $proc.ProcessName
    Width = $rect.Right - $rect.Left
    Height = $rect.Bottom - $rect.Top
  })
  return $true
}
[Win32Cap]::EnumWindows($callback, [IntPtr]::Zero) | Out-Null

$i = 0
foreach ($t in $targets) {
  if ($t.Width -lt 40 -or $t.Height -lt 40) { continue }
  $name = "{0:d2}_{1}_{2}x{3}" -f $i, ($t.Class -replace '[^\w\-]', '_'), $t.Width, $t.Height
  if ($t.Title) { $name = "{0:d2}_{1}" -f $i, (($t.Title -replace '[^\w\- ]', '_').Trim() -replace '\s+', '-') }
  $path = Join-Path $out "$name.png"
  if (Capture-Hwnd $t.Hwnd $path) {
    Write-Host "Saved $path  ($($t.Process) / $($t.Class) / $($t.Width)x$($t.Height))"
    $i++
  }
}

if ($i -eq 0) {
  Write-Warning "No matching windows captured. Start the resident/manager first, or pass -ProcessName."
  exit 1
}
Write-Host "Captured $i window(s) into $out"
