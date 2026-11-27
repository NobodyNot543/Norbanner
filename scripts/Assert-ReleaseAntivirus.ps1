# Analyze release bytes without executing them or changing Defender preferences.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string[]]$Path,
    [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference = 'Stop'

function Get-ReleaseDefenderScanner {
    $platform = Join-Path $env:ProgramData 'Microsoft/Windows Defender/Platform'
    $latest = Get-ChildItem -LiteralPath $platform -Directory -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($latest) {
        $candidate = Join-Path $latest.FullName 'MpCmdRun.exe'
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    }
    $fallback = Join-Path $env:ProgramFiles 'Windows Defender/MpCmdRun.exe'
    if (-not (Test-Path -LiteralPath $fallback -PathType Leaf)) {
        throw 'Microsoft Defender scanner is unavailable. Use a Windows runner with Defender installed.'
    }
    $fallback
}

function Invoke-ReleaseDefenderScan([string]$Scanner, [string]$File) {
    # This flag affects only this analysis: it ignores the hosted runner's broad
    # build exclusions and scans archives. Defender itself remains enabled.
    # Detected files are never executed or published; a nonzero result fails CI.
    $PSNativeCommandUseErrorActionPreference = $false
    & $Scanner -Scan -ScanType 3 -File $File -DisableRemediation | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "Defender rejected $File or could not complete its analysis (exit $LASTEXITCODE)."
    }
}

function Assert-ReleaseAntivirus([string[]]$Path, [string]$ReportDirectory) {
    $report = [ordered]@{ passed = $false; scannedAt = [DateTime]::UtcNow.ToString('o'); files = @() }
    New-Item -ItemType Directory -Path $ReportDirectory -Force | Out-Null
    try {
        $status = Get-MpComputerStatus -ErrorAction Stop
        if (-not $status.AMServiceEnabled -or -not $status.AntivirusEnabled) {
            throw 'Microsoft Defender is unavailable or disabled. Antivirus verification cannot be skipped.'
        }
        $report.engineVersion = [string]$status.AMEngineVersion
        $report.signatureVersion = [string]$status.AntivirusSignatureVersion
        if ($null -eq $status.AntivirusSignatureAge -or $status.AntivirusSignatureAge -gt 1 -or
            [string]::IsNullOrWhiteSpace($report.engineVersion) -or [string]::IsNullOrWhiteSpace($report.signatureVersion)) {
            throw 'Defender definitions are older than one day. Update signatures on the build runner and retry.'
        }
        $scanner = Get-ReleaseDefenderScanner
        foreach ($file in $Path) {
            $resolved = (Resolve-Path -LiteralPath $file -ErrorAction Stop).Path
            $hash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash
            $signature = Get-AuthenticodeSignature -LiteralPath $resolved
            $row = [ordered]@{
                file = [IO.Path]::GetFileName($resolved); sha256 = $hash
                fileVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($resolved).FileVersion
                authenticode = [string]$signature.Status
                publisher = if ($signature.SignerCertificate) { $signature.SignerCertificate.Subject } else { $null }
                passed = $false
            }
            $report.files += $row
            Invoke-ReleaseDefenderScan -Scanner $scanner -File $resolved
            # A removed, quarantined or changed file cannot inherit a clean result.
            if ((Get-FileHash -LiteralPath $resolved -Algorithm SHA256 -ErrorAction Stop).Hash -ne $hash) {
                throw "Release bytes changed during antivirus verification: $resolved"
            }
            $row.passed = $true
        }
        $report.passed = $true
    } catch {
        $report.error = $_.Exception.Message
        throw
    } finally {
        $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'antivirus-report.json') -Encoding utf8NoBOM
    }
}

# Dot-sourcing exposes the orchestration for offline tests; normal runs scan.
if ($MyInvocation.InvocationName -ne '.') { Assert-ReleaseAntivirus @PSBoundParameters }
