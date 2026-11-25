param([string]$Label, [string]$Jvm = "", [int]$N = 1, [switch]$Off)
$env:IKA_BENCH = 'Linggango'
if ($Jvm) { $env:IKA_BENCH_JVM = $Jvm } else { Remove-Item Env:IKA_BENCH_JVM -ErrorAction SilentlyContinue }
if ($Off) { $env:IKA_BOOST_OFF = '1' } else { Remove-Item Env:IKA_BOOST_OFF -ErrorAction SilentlyContinue }
$o = "$env:APPDATA\IkaLauncher\instances\Linggango\.minecraft\logs\ika-launcher-output.log"
for ($i = 1; $i -le $N; $i++) {
  Get-CimInstance Win32_Process -Filter "Name='java.exe' or Name='javaw.exe'" | ? { $_.CommandLine -match 'Linggango|crash_assistant' } | % { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  $r = cargo test --release --bin ika-wgpu-wallpaper-bridge bench_launch -- --ignored --nocapture 2>&1 | Select-String 'BENCH|PHASES|run_in|panicked' | % Line
  $b = Select-String $o -Pattern '\[IkaBoost\] (cache:|saved|package index|unknown|could)' | % { $_.Line -replace '^.*?\[IkaBoost\]','' -replace '\]\]>.*','' } | select -Unique
  "$Label#$i " + ($r -join ' | ') + ' || ' + ($b -join ' ; ')
  Add-Content "$PSScriptRoot\..\..\scratch\lab.log" ("$(Get-Date -f HH:mm) $Label#$i " + ($r -join ' | ') + ' || ' + ($b -join ' ; '))
}
