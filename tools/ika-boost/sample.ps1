param([string]$Out, [int]$Seconds = 80, [int]$Ms = 300)
New-Item -ItemType Directory -Force $Out | Out-Null
Get-ChildItem $Out -Filter *.txt | Remove-Item
$js = "C:\Program Files\Java\jdk-21\bin\jstack.exe"
$start = Get-Date; $p = $null
while (((Get-Date) - $start).TotalSeconds -lt $Seconds) {
  if (-not $p) { $p = Get-CimInstance Win32_Process -Filter "Name='java.exe' or Name='javaw.exe'" | ? { $_.CommandLine -match 'Linggango' -and $_.CommandLine -notmatch 'crash_assistant' } | select -First 1; if ($p) { $t0 = Get-Date } }
  if ($p) { $ms = [int]((Get-Date) - $t0).TotalMilliseconds; & $js $p.ProcessId > "$Out\$ms.txt" 2>&1 }
  Start-Sleep -Milliseconds $Ms
}
