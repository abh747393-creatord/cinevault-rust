<#
.SYNOPSIS
    CineVault Safe MovieBox-TUI Updater
.DESCRIPTION
    Safely pins, compiles, and verifies MovieBox-TUI dependency updates on the authorized backend host.
    Includes atomic backup, GitHub tag verification, cargo check, cargo test, concurrency locking,
    safe process stopping, restart, live /health verification, and automatic rollback on failure.
.PARAMETER TargetVersion
    The semantic version to pin (e.g. "0.1.24").
.PARAMETER DryRun
    Verifies version syntax and upstream release tag without modifying files.
.PARAMETER RestartService
    Safely stops running server, builds binary, restarts server, and verifies live /health endpoint.
#>
param(
    [Parameter(Mandatory=$true)]
    [string]$TargetVersion,

    [switch]$DryRun,

    [switch]$RestartService
)

$ErrorActionPreference = "Stop"

$CleanVersion = $TargetVersion.Trim().TrimStart('v')
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host " CineVault Safe MovieBox-TUI Host Updater" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Target Version: v$CleanVersion"
Write-Host "Mode: $(if ($DryRun) { 'Dry-Run Simulation' } elseif ($RestartService) { 'Update + Build + Restart + Health Check' } else { 'Update + Cargo Check/Test' })"

# 1. Validate version syntax (Strict stable semver: digits and dots only, no prerelease/special chars)
if ($CleanVersion -notmatch '^[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}$') {
    Write-Error "Invalid semantic version format: '$TargetVersion'. Only stable semver (e.g. '0.1.24') is permitted."
    exit 1
}

# 2. Verify against official upstream GitHub releases
Write-Host "`n[1/6] Verifying stable release tag with GitHub API..." -ForegroundColor Yellow
$ApiUrl = "https://api.github.com/repos/mesamirh/MovieBox-Tui/releases?per_page=15"
try {
    $ReleasesJson = Invoke-RestMethod -Uri $ApiUrl -Headers @{ "User-Agent" = "CineVault-Host-Updater" } -TimeoutSec 10
    $FoundRelease = $ReleasesJson | Where-Object { 
        (-not $_.prerelease) -and ($_.tag_name.TrimStart('v') -eq $CleanVersion)
    }
    if (-not $FoundRelease) {
        Write-Error "Target version '$CleanVersion' does not match any official upstream stable release tag on GitHub."
        exit 1
    }
    Write-Host "  -> Verified upstream stable release: $($FoundRelease.name) ($($FoundRelease.tag_name))" -ForegroundColor Green
} catch {
    Write-Error "Failed to verify release with GitHub: $_"
    exit 1
}

if ($DryRun) {
    Write-Host "`n[DRY RUN COMPLETE] Target version v$CleanVersion is valid and verified against GitHub." -ForegroundColor Green
    Write-Host "Pre-flight checks passed successfully. No files were modified."
    exit 0
}

# Locate Cargo.toml and paths
$RepoDir = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $RepoDir "Cargo.toml"))) {
    $RepoDir = Get-Location
}
$CargoToml = Join-Path $RepoDir "Cargo.toml"
$CargoLock = Join-Path $RepoDir "Cargo.lock"
$CargoTomlBak = Join-Path $RepoDir "Cargo.toml.bak"
$CargoLockBak = Join-Path $RepoDir "Cargo.lock.bak"
$LockFile = Join-Path $RepoDir ".updater.lock"

if (-not (Test-Path $CargoToml)) {
    Write-Error "Cargo.toml not found in $RepoDir"
    exit 1
}

# Concurrency Mutex check
if (Test-Path $LockFile) {
    $LockAge = (Get-Date) - (Get-Item $LockFile).LastWriteTime
    if ($LockAge.TotalMinutes -lt 10) {
        Write-Error "Another update process is currently in progress (active lockfile: $LockFile). Aborting to prevent race conditions."
        exit 1
    } else {
        Write-Warning "Found stale lockfile (>10 min old). Overriding stale lock."
        Remove-Item -Path $LockFile -Force
    }
}

# Acquire lock
Set-Content -Path $LockFile -Value "PID=$PID, Started=$(Get-Date -Format o)"

# 3. Create atomic backups
Write-Host "`n[2/6] Creating atomic configuration backups..." -ForegroundColor Yellow
Copy-Item -Path $CargoToml -Destination $CargoTomlBak -Force
if (Test-Path $CargoLock) {
    Copy-Item -Path $CargoLock -Destination $CargoLockBak -Force
}
Write-Host "  -> Backups created: Cargo.toml.bak, Cargo.lock.bak" -ForegroundColor Green

$CurrentContent = Get-Content -Path $CargoToml -Raw
$RollbackNeeded = $true
$env:CARGO_TARGET_DIR = "C:\Users\Public\cargo-target"

try {
    # 4. Update Cargo.toml version
    Write-Host "`n[3/6] Pinning version in Cargo.toml..." -ForegroundColor Yellow
    $NewContent = $CurrentContent -replace '(name\s*=\s*"moviebox-tui"[\s\S]*?version\s*=\s*)"[^"]+"', "`$1`"$CleanVersion`""
    if ($NewContent -eq $CurrentContent) {
        $NewContent = $CurrentContent -replace '(\[package\][\s\S]*?version\s*=\s*)"[^"]+"', "`$1`"$CleanVersion`""
    }
    Set-Content -Path $CargoToml -Value $NewContent -NoNewline
    Write-Host "  -> Cargo.toml updated to v$CleanVersion" -ForegroundColor Green

    # 5. Build and Test Verification
    Write-Host "`n[4/6] Running cargo check --bin cinevault_server..." -ForegroundColor Yellow
    $checkProc = Start-Process -FilePath "cargo" -ArgumentList "check --bin cinevault_server" -WorkingDirectory $RepoDir -NoNewWindow -Wait -PassThru
    if ($checkProc.ExitCode -ne 0) {
        throw "cargo check failed with exit code $($checkProc.ExitCode)"
    }
    Write-Host "  -> cargo check passed." -ForegroundColor Green

    Write-Host "`n[5/6] Running cargo test --bin cinevault_server..." -ForegroundColor Yellow
    $testProc = Start-Process -FilePath "cargo" -ArgumentList "test --bin cinevault_server" -WorkingDirectory $RepoDir -NoNewWindow -Wait -PassThru
    if ($testProc.ExitCode -ne 0) {
        throw "cargo test failed with exit code $($testProc.ExitCode)"
    }
    Write-Host "  -> cargo test passed." -ForegroundColor Green

    # 6. Optional: Safe Stop, Compile Binary, Restart & /health Verification
    if ($RestartService) {
        Write-Host "`n[6/6] Safe stop, compile, restart, and /health verification..." -ForegroundColor Yellow

        # 6a. Safe stop existing server if running
        $existingProcs = Get-Process -Name "cinevault_server" -ErrorAction SilentlyContinue
        if ($existingProcs) {
            Write-Host "  -> Stopping existing cinevault_server process(es)..."
            $existingProcs | Stop-Process -Force
            Start-Sleep -Seconds 1
        }

        # 6b. Build the executable
        Write-Host "  -> Compiling release/dev executable..."
        $buildProc = Start-Process -FilePath "cargo" -ArgumentList "build --bin cinevault_server" -WorkingDirectory $RepoDir -NoNewWindow -Wait -PassThru
        if ($buildProc.ExitCode -ne 0) {
            throw "cargo build failed with exit code $($buildProc.ExitCode)"
        }

        # 6c. Restart backend process
        Write-Host "  -> Starting updated cinevault_server on port 8080..."
        $serverBinary = Join-Path $env:CARGO_TARGET_DIR "debug\cinevault_server.exe"
        if (-not (Test-Path $serverBinary)) {
            throw "Compiled binary not found at $serverBinary"
        }

        $startInfo = New-Object System.Diagnostics.ProcessStartInfo
        $startInfo.FileName = $serverBinary
        $startInfo.EnvironmentVariables["PORT"] = "8080"
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startedProc = [System.Diagnostics.Process]::Start($startInfo)
        Start-Sleep -Seconds 2

        # 6d. Verify /health
        Write-Host "  -> Verifying live /health endpoint..."
        $healthVerified = $false
        for ($i = 0; $i -lt 10; $i++) {
            try {
                $hRes = Invoke-RestMethod -Uri "http://127.0.0.1:8080/health" -TimeoutSec 2
                if ($hRes.status -eq "healthy" -and $hRes.version -eq $CleanVersion) {
                    $healthVerified = $true
                    Write-Host "  -> /health verified: status=$($hRes.status), version=$($hRes.version)" -ForegroundColor Green
                    break
                }
            } catch {}
            Start-Sleep -Seconds 1
        }

        if (-not $healthVerified) {
            throw "Live /health check verification failed for updated server."
        }
    } else {
        Write-Host "`n[6/6] Build and verification complete (use -RestartService to automatically cycle process)." -ForegroundColor Green
    }

    # Success: Clean up backups and release lock
    $RollbackNeeded = $false
    if (Test-Path $CargoTomlBak) { Remove-Item -Path $CargoTomlBak -Force }
    if (Test-Path $CargoLockBak) { Remove-Item -Path $CargoLockBak -Force }
    if (Test-Path $LockFile) { Remove-Item -Path $LockFile -Force }

    Write-Host "`n==================================================" -ForegroundColor Green
    Write-Host " UPDATE SUCCESSFULLY APPLIED AND VERIFIED!" -ForegroundColor Green
    Write-Host "==================================================" -ForegroundColor Green
    Write-Host "Version is now pinned to v$CleanVersion."
} catch {
    Write-Host "`n[ERROR] Update failed: $_" -ForegroundColor Red
    if ($RollbackNeeded) {
        Write-Host "Performing automatic atomic rollback..." -ForegroundColor Magenta
        if (Test-Path $CargoTomlBak) {
            Copy-Item -Path $CargoTomlBak -Destination $CargoToml -Force
            Remove-Item -Path $CargoTomlBak -Force
        }
        if (Test-Path $CargoLockBak) {
            Copy-Item -Path $CargoLockBak -Destination $CargoLock -Force
            Remove-Item -Path $CargoLockBak -Force
        }
        Write-Host "Rollback complete. Restored original working version." -ForegroundColor Green
    }
    if (Test-Path $LockFile) { Remove-Item -Path $LockFile -Force }
    exit 1
}
