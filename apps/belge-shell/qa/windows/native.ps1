# GölgeDosya — Windows yerel yardımcıları (Kapı 6).
#
# CDP sayfanın İÇİNİ görür; bu betik DIŞINI: gerçek klavye/fare girdisi,
# pencere görüntüsü (yerel başlık çubuğu dahil), tarayıcının kendi sağ tık
# menüsü (UI Automation) ve Gezgin'in hangi klasörü açtığı (Shell.Application).
# Her çağrı tek iş yapar ve stdout'a tek satır JSON yazar.
#
# PowerShell değişken adlarında büyük/küçük harf ayırmaz: `[int]$H` diye bir
# parametre, gövdedeki `$h = <pencere tanıtıcısı>` atamasını tamsayıya çevirip
# yüksekliğe yazıyordu (32728 px'lik pencere); `[int]$W` de `foreach ($w in
# <Gezgin pencereleri>)` döngüsünü düşürüyordu. Parametre adları bu yüzden uzun.
param(
  [Parameter(Mandatory)][string]$Action,
  [int]$ProcessId,
  [string]$Keys,
  [int]$X, [int]$Y, [int]$Width, [int]$Height,
  [string]$Path
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class GdU32 {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
}
"@
[void][GdU32]::SetProcessDPIAware()

function Out-Json($o) { $o | ConvertTo-Json -Compress -Depth 6 }

function Get-Hwnd {
  $p = Get-Process -Id $ProcessId -ErrorAction Stop
  for ($i = 0; $i -lt 40 -and $p.MainWindowHandle -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 250; $p.Refresh() }
  if ($p.MainWindowHandle -eq [IntPtr]::Zero) { throw "ana pencere tanıtıcısı yok (pid $ProcessId)" }
  $p.MainWindowHandle
}

function Get-Client($h) {
  $r = New-Object GdU32+RECT; [void][GdU32]::GetClientRect($h, [ref]$r)
  $o = New-Object GdU32+POINT; [void][GdU32]::ClientToScreen($h, [ref]$o)
  @{ x = $o.X; y = $o.Y; w = $r.R - $r.L; h = $r.B - $r.T }
}

function Focus($h) {
  if ([GdU32]::GetForegroundWindow() -eq $h) { return $true }
  # SW_SHOW: görünür yap ama büyütülmüş pencereyi küçültme (SW_RESTORE küçültürdü).
  [void][GdU32]::ShowWindow($h, 5)
  # Arka plandaki bir süreç ön plana pencere getiremez; bir girdi olayı bu
  # kilidi açar. Alt KULLANILMAZ: bırakılan Alt pencereyi menü kipine sokar ve
  # sonraki tuşu (F5) yutabilirdi. F24 hiçbir şeye bağlı değildir.
  [GdU32]::keybd_event(0x87, 0, 0, [UIntPtr]::Zero); [GdU32]::keybd_event(0x87, 0, 2, [UIntPtr]::Zero)
  [void][GdU32]::SetForegroundWindow($h)
  Start-Sleep -Milliseconds 300
  [GdU32]::GetForegroundWindow() -eq $h
}

$VK = @{ CTRL = 0x11; SHIFT = 0x10; ALT = 0x12; F5 = 0x74; F24 = 0x87; R = 0x52; S = 0x53; ESC = 0x1B; LEFT = 0x25 }

switch ($Action) {
  'window' {
    $h = Get-Hwnd
    $r = New-Object GdU32+RECT; [void][GdU32]::GetWindowRect($h, [ref]$r)
    Out-Json @{ hwnd = [int64]$h; window = @{ x = $r.L; y = $r.T; w = $r.R - $r.L; h = $r.B - $r.T }; client = (Get-Client $h); zoomed = [GdU32]::IsZoomed($h) }
  }
  'focus' { $h = Get-Hwnd; Out-Json @{ foreground = (Focus $h) } }
  'move' {
    $h = Get-Hwnd; [void][GdU32]::ShowWindow($h, 9); Start-Sleep -Milliseconds 200
    [void][GdU32]::MoveWindow($h, $X, $Y, $Width, $Height, $true); Start-Sleep -Milliseconds 600
    Out-Json @{ client = (Get-Client $h) }
  }
  'maximize' { $h = Get-Hwnd; [void][GdU32]::ShowWindow($h, 3); Start-Sleep -Milliseconds 800; Out-Json @{ zoomed = [GdU32]::IsZoomed($h); client = (Get-Client $h) } }
  'key' {
    # -Keys "CTRL+SHIFT+R" biçiminde. Değiştiriciler basılır, tuş vurulur, ters sırada bırakılır.
    $h = Get-Hwnd; $fg = Focus $h
    $parts = $Keys.ToUpperInvariant().Split('+')
    $codes = @($parts | ForEach-Object { if ($VK.ContainsKey($_)) { $VK[$_] } else { [int][char]$_ } })
    foreach ($c in $codes) { [GdU32]::keybd_event([byte]$c, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 30 }
    [array]::Reverse($codes)
    foreach ($c in $codes) { [GdU32]::keybd_event([byte]$c, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 30 }
    Out-Json @{ foreground = $fg; keys = $Keys }
  }
  'rightclick' {
    # X/Y: istemci alanına göre (CSS pikseli × DPR hesabı çağıranda).
    $h = Get-Hwnd; $fg = Focus $h; $c = Get-Client $h
    [void][GdU32]::SetCursorPos($c.x + $X, $c.y + $Y); Start-Sleep -Milliseconds 150
    [GdU32]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60
    [GdU32]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    Out-Json @{ foreground = $fg; screen = @{ x = $c.x + $X; y = $c.y + $Y } }
  }
  'click' {
    $h = Get-Hwnd; $fg = Focus $h; $c = Get-Client $h
    [void][GdU32]::SetCursorPos($c.x + $X, $c.y + $Y); Start-Sleep -Milliseconds 150
    [GdU32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60
    [GdU32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Out-Json @{ foreground = $fg }
  }
  'shot' {
    # Pencerenin tamamı (yerel başlık çubuğu dahil). -Width/-Height verilirse ekrandan o dikdörtgen.
    if ($Width -gt 0) { $rx = $X; $ry = $Y; $rw = $Width; $rh = $Height }
    else { $h = Get-Hwnd; $r = New-Object GdU32+RECT; [void][GdU32]::GetWindowRect($h, [ref]$r); $rx = $r.L; $ry = $r.T; $rw = $r.R - $r.L; $rh = $r.B - $r.T }
    $bmp = New-Object System.Drawing.Bitmap $rw, $rh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($rx, $ry, 0, 0, $bmp.Size)
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    Out-Json @{ path = $Path; w = $rw; h = $rh }
  }
  'menus' {
    # WebView2'nin bağlam menüsü ayrı bir üst düzey pencere: msedgewebview2.exe sürecine ait.
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $A = [System.Windows.Automation.AutomationElement]
    $pids = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue | ForEach-Object { $_.Id }) + @($ProcessId)
    $items = @(); $menus = 0
    foreach ($top in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
      if ($pids -notcontains $top.Current.ProcessId) { continue }
      $isMenu = $top.Current.ControlType -eq [System.Windows.Automation.ControlType]::Menu
      $found = $top.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem)))
      if ($isMenu -or $found.Count -gt 0) { $menus++ }
      foreach ($m in $found) { $items += $m.Current.Name }
    }
    Out-Json @{ menus = $menus; items = $items }
  }
  'explorer' {
    # Açık Gezgin pencereleri: hangi klasör, içinde ne seçili.
    $shell = New-Object -ComObject Shell.Application
    $wins = @()
    foreach ($w in @($shell.Windows())) {
      try {
        $folder = $w.Document.Folder.Self.Path
        $sel = @($w.Document.SelectedItems() | ForEach-Object { $_.Path })
        $wins += @{ path = $folder; selected = $sel }
      } catch { }
    }
    Out-Json @{ windows = $wins }
  }
  'clip-file' {
    # Gezgin'de "Kopyala" ile aynı: panoya dosya listesi (CF_HDROP).
    Set-Clipboard -LiteralPath $Path
    Out-Json @{ clipboard = $Path }
  }
  'close-explorer' {
    $shell = New-Object -ComObject Shell.Application
    foreach ($w in @($shell.Windows())) { try { $w.Quit() } catch { } }
    Out-Json @{ closed = $true }
  }
  'toplevel' {
    # Uygulamanın ve WebView2'nin üst düzey pencereleri: Farklı Kaydet iletişim
    # kutusu (#32770) ya da tarayıcı menüsü açıldıysa burada görünür.
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $A = [System.Windows.Automation.AutomationElement]
    $pids = @(Get-Process msedgewebview2 -ErrorAction SilentlyContinue | ForEach-Object { $_.Id }) + @($ProcessId)
    $wins = @()
    foreach ($top in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
      if ($pids -notcontains $top.Current.ProcessId) { continue }
      $wins += @{ name = $top.Current.Name; cls = $top.Current.ClassName; type = $top.Current.ControlType.ProgrammaticName }
    }
    Out-Json @{ windows = $wins }
  }
  'screen' {
    Add-Type -AssemblyName System.Windows.Forms
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    Out-Json @{ path = $Path; w = $b.Width; h = $b.Height }
  }
  'diff' {
    # İki aynı boyutlu PNG arasında farklı piksel sayısı: menü açıldı mı?
    Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class GdDiff {
  public static int Count(string a, string b) {
    using (var x = new Bitmap(a)) using (var y = new Bitmap(b)) {
      if (x.Width != y.Width || x.Height != y.Height) return -1;
      var r = new Rectangle(0, 0, x.Width, x.Height);
      var dx = x.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
      var dy = y.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
      int n = dx.Stride * x.Height; var px = new byte[n]; var py = new byte[n];
      Marshal.Copy(dx.Scan0, px, 0, n); Marshal.Copy(dy.Scan0, py, 0, n);
      x.UnlockBits(dx); y.UnlockBits(dy);
      int diff = 0;
      for (int i = 0; i < n; i += 4) {
        if (Math.Abs(px[i] - py[i]) + Math.Abs(px[i + 1] - py[i + 1]) + Math.Abs(px[i + 2] - py[i + 2]) > 24) diff++;
      }
      return diff;
    }
  }
}
"@
    $pair = $Path.Split('|')
    Out-Json @{ changed = [GdDiff]::Count($pair[0], $pair[1]) }
  }
  'darkframe' {
    # Yerel başlık çubuğu koyu mu: DWMWA_USE_IMMERSIVE_DARK_MODE (20).
    Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class GdDwm {
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out int value, int size);
}
"@
    $h = Get-Hwnd; $v = 0
    $hr = [GdDwm]::DwmGetWindowAttribute($h, 20, [ref]$v, 4)
    Out-Json @{ hr = $hr; value = $v; dark = ($hr -eq 0 -and $v -ne 0) }
  }
  'resolution' {
    # Barındırılan runner'ın ekranı 1024x768: 1280x800'lük pencere ekrana
    # sığmaz, ekran görüntüsü kırpılır. Birincil ekranı -Width x -Height'a
    # almayı DENER; sonucu (önce/sonra) raporlar, başarısızlığı gizlemez.
    Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class GdDisp {
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  public struct DEVMODE {
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
    public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
    public int dmFields, dmPositionX, dmPositionY, dmDisplayOrientation, dmDisplayFixedOutput;
    public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
    public short dmLogPixels;
    public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
    public int dmICMMethod, dmICMIntent, dmMediaType, dmDitherType, dmReserved1, dmReserved2, dmPanningWidth, dmPanningHeight;
  }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool EnumDisplaySettings(string dev, int mode, ref DEVMODE dm);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int ChangeDisplaySettings(ref DEVMODE dm, int flags);
  public static int[] Current() {
    var dm = new DEVMODE(); dm.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
    EnumDisplaySettings(null, -1, ref dm); return new[] { dm.dmPelsWidth, dm.dmPelsHeight };
  }
  public static int Set(int w, int h) {
    var dm = new DEVMODE(); dm.dmSize = (short)Marshal.SizeOf(typeof(DEVMODE));
    if (!EnumDisplaySettings(null, -1, ref dm)) return -100;
    dm.dmPelsWidth = w; dm.dmPelsHeight = h; dm.dmFields = 0x80000 | 0x100000;
    return ChangeDisplaySettings(ref dm, 0);
  }
}
"@
    $before = [GdDisp]::Current()
    $code = [GdDisp]::Set($Width, $Height); Start-Sleep -Milliseconds 1500
    $after = [GdDisp]::Current()
    Out-Json @{ before = $before; after = $after; result = $code }
  }
  default { throw "bilinmeyen eylem: $Action" }
}
