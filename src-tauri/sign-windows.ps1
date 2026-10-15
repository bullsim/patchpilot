# Authenticode-signs one file with Azure Trusted Signing.
# Invoked by the Tauri bundler for every .exe / .msi it produces
# (tauri.conf.json -> bundle.windows.signCommand), so the updater's
# minisign .sig is computed over the *signed* installer.
#
# Silently no-ops when the Trusted Signing settings are absent, so local
# builds and forks still work — they just produce unsigned binaries.
#
# Required environment (all set by .github/workflows/release.yml from secrets):
#   AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET   service principal
#   TRUSTED_SIGNING_ENDPOINT   e.g. https://weu.codesigning.azure.net
#   TRUSTED_SIGNING_ACCOUNT    the Trusted Signing account name
#   TRUSTED_SIGNING_PROFILE    the certificate profile name
#   TRUSTED_SIGNING_DLIB       path to Azure.CodeSigning.Dlib.dll
# See docs/SIGNING.md.

param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = "Stop"

$needed = @(
    "AZURE_TENANT_ID", "AZURE_CLIENT_ID", "AZURE_CLIENT_SECRET",
    "TRUSTED_SIGNING_ENDPOINT", "TRUSTED_SIGNING_ACCOUNT",
    "TRUSTED_SIGNING_PROFILE", "TRUSTED_SIGNING_DLIB"
)
$missing = $needed | Where-Object { [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_)) }
if ($missing) {
    Write-Host "sign-windows: Trusted Signing not configured ($($missing -join ', ')) - leaving unsigned: $File"
    exit 0
}

if (-not (Test-Path $File)) { throw "sign-windows: file not found: $File" }
if (-not (Test-Path $env:TRUSTED_SIGNING_DLIB)) { throw "sign-windows: dlib not found: $env:TRUSTED_SIGNING_DLIB" }

# signtool.exe from the newest installed Windows SDK.
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\$arch\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object { [version]($_.Directory.Parent.Name) } -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) { throw "sign-windows: signtool.exe not found (Windows SDK missing)" }

# Metadata file the dlib reads to know which account/profile to sign with.
$meta = Join-Path $env:TEMP "patchpilot-trusted-signing.json"
@{
    Endpoint               = $env:TRUSTED_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:TRUSTED_SIGNING_ACCOUNT
    CertificateProfileName = $env:TRUSTED_SIGNING_PROFILE
} | ConvertTo-Json | Set-Content -Path $meta -Encoding ascii

Write-Host "sign-windows: signing $File"
& $signtool sign /v /fd SHA256 /tr "http://timestamp.acs.microsoft.com" /td SHA256 `
    /dlib $env:TRUSTED_SIGNING_DLIB /dmdf $meta $File
if ($LASTEXITCODE -ne 0) { throw "sign-windows: signtool failed with exit code $LASTEXITCODE" }

& $signtool verify /pa /v $File | Out-Null
if ($LASTEXITCODE -ne 0) { throw "sign-windows: signature verification failed" }
Write-Host "sign-windows: signed OK"
