# Screenshot of a process's main window: shot.ps1 -ProcessId <pid> -Out <png>
# Uses PrintWindow(PW_RENDERFULLCONTENT), which renders the window even when
# it is covered by other windows — so taking a shot never steals focus.
param([int]$ProcessId, [string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[W]::SetProcessDPIAware() | Out-Null
$h = (Get-Process -Id $ProcessId).MainWindowHandle
if ($h -eq [IntPtr]::Zero) { throw "process $ProcessId has no main window" }
$win = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$win) | Out-Null
$vis = New-Object W+RECT; [W]::DwmGetWindowAttribute($h, 9, [ref]$vis, 16) | Out-Null  # DWMWA_EXTENDED_FRAME_BOUNDS
$full = New-Object System.Drawing.Bitmap ($win.R - $win.L), ($win.B - $win.T)
$g = [System.Drawing.Graphics]::FromImage($full)
$hdc = $g.GetHdc()
[W]::PrintWindow($h, $hdc, 2) | Out-Null  # PW_RENDERFULLCONTENT
$g.ReleaseHdc($hdc)
# Crop the invisible resize borders: keep only the visible frame.
$crop = New-Object System.Drawing.Rectangle ($vis.L - $win.L), ($vis.T - $win.T), ($vis.R - $vis.L), ($vis.B - $vis.T)
$full.Clone($crop, $full.PixelFormat).Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"$($crop.Width) x $($crop.Height) -> $Out"
