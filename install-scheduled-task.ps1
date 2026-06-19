<#
  Registers a daily scheduled task that runs PatchPilot headlessly.
  Run from an ELEVATED PowerShell prompt.

  Examples:
    .\install-scheduled-task.ps1                       # 03:00 daily, All
    .\install-scheduled-task.ps1 -Time 02:30 -Mode Software
    .\install-scheduled-task.ps1 -Remove               # delete the task
#>
param(
    [string]$Time = "03:00",
    [ValidateSet('All','Software','Firmware')]
    [string]$Mode = "All",
    [switch]$Remove
)

$ErrorActionPreference = 'Stop'
$TaskName = "PatchPilot_DailyUpdates"

if ($Remove) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
    Write-Host "Removed scheduled task '$TaskName'." -ForegroundColor Yellow
    return
}

# Locate the built exe (release first, then debug).
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$candidates = @(
    Join-Path $root "src-tauri\target\release\patchpilot.exe"
    Join-Path $root "src-tauri\target\debug\patchpilot.exe"
    Join-Path $env:LOCALAPPDATA "Programs\PatchPilot\patchpilot.exe"
)
$exe = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $exe) {
    throw "patchpilot.exe not found. Run 'npm run tauri build' first, or install the MSI."
}

$action  = New-ScheduledTaskAction  -Execute $exe -Argument "--silent --mode $Mode"
$trigger = New-ScheduledTaskTrigger -Daily -At $Time
$settings = New-ScheduledTaskSettingsSet -StartWhenAvailable `
    -DontStopOnIdleEnd -ExecutionTimeLimit (New-TimeSpan -Hours 2)
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" `
    -LogonType S4U -RunLevel Highest

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger `
    -Settings $settings -Principal $principal -Force | Out-Null

Write-Host "Scheduled '$TaskName' daily at $Time (mode: $Mode)" -ForegroundColor Green
Write-Host "Target: $exe"
