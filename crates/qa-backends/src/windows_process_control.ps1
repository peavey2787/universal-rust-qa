$ErrorActionPreference = "Stop"
$rootId = [uint32]$env:QA_PROCESS_CONTROL_PID
$mode = $env:QA_PROCESS_CONTROL_MODE

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class QaProcessControl {
    [DllImport("ntdll.dll")]
    public static extern int NtSuspendProcess(IntPtr processHandle);
    [DllImport("ntdll.dll")]
    public static extern int NtResumeProcess(IntPtr processHandle);
}
"@

$rows = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
$ids = [System.Collections.Generic.HashSet[uint32]]::new()
[void]$ids.Add($rootId)
do {
    $changed = $false
    foreach ($row in $rows) {
        if ($ids.Contains([uint32]$row.ParentProcessId) -and $ids.Add([uint32]$row.ProcessId)) {
            $changed = $true
        }
    }
} while ($changed)

$otherIds = @($ids | Where-Object { $_ -ne $rootId })
if ($mode -eq "suspend") {
    $orderedIds = @($rootId) + $otherIds
} elseif ($mode -eq "resume") {
    $orderedIds = $otherIds + @($rootId)
} elseif ($mode -eq "terminate") {
    $orderedIds = $otherIds + @($rootId)
} elseif ($mode -eq "terminate-descendants") {
    $orderedIds = $otherIds
} else {
    exit 2
}

$rootChanged = $false
$anyChanged = $false
foreach ($id in $orderedIds) {
    $process = Get-Process -Id $id -ErrorAction SilentlyContinue
    if ($null -eq $process) {
        continue
    }
    if ($mode -eq "terminate" -or $mode -eq "terminate-descendants") {
        try {
            Stop-Process -Id $id -Force -ErrorAction Stop
            $status = 0
        } catch {
            $status = -1
        }
    } elseif ($mode -eq "suspend") {
        $status = [QaProcessControl]::NtSuspendProcess($process.Handle)
    } else {
        $status = [QaProcessControl]::NtResumeProcess($process.Handle)
    }
    if ($status -ge 0) {
        $anyChanged = $true
        if ($id -eq $rootId) {
            $rootChanged = $true
        }
    }
}

if ($mode -eq "terminate" -or $mode -eq "terminate-descendants") {
    if (-not $anyChanged) {
        exit 1
    }
} elseif (-not $rootChanged) {
    exit 1
}
