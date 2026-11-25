param([string]$Label, [string]$Java, [string]$Jvm = "")
$env:IKA_BENCH_CFG = "JavaPath=$Java;OverrideJavaLocation=true"
& "$PSScriptRoot\lab.ps1" -Label "$Label-warm" -Jvm $Jvm -N 2 | Out-Null
& "$PSScriptRoot\lab.ps1" -Label $Label -Jvm $Jvm -N 2
Remove-Item Env:IKA_BENCH_CFG
