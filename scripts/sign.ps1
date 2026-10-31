param([Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Files)
$ErrorActionPreference = "Stop"

if (-not $env:SIGNING_PFX_BASE64) { Write-Host "No certificate: signing skipped."; exit 0 }

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw "signtool.exe introuvable (Windows SDK)" }

$pfx = Join-Path $env:RUNNER_TEMP "signing.pfx"
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:SIGNING_PFX_BASE64))
try {
    foreach ($f in $Files) {
        & $signtool.FullName sign /fd SHA256 /td SHA256 /tr "http://timestamp.digicert.com" `
            /f $pfx /p $env:SIGNING_PFX_PASSWORD /d "MarkdownShot" $f
        if ($LASTEXITCODE -ne 0) { throw "signing failed for $f" }
        & $signtool.FullName verify /pa $f
    }
} finally {
    Remove-Item $pfx -Force -ErrorAction SilentlyContinue
}
