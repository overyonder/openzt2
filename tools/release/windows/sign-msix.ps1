param([Parameter(Mandatory=$true)][string]$PackagePath)
$ErrorActionPreference = 'Stop'
$certificate = $null
$temporaryPfx = Join-Path $env:RUNNER_TEMP 'openzt2-signing.pfx'
try {
    if ($env:OPENZT2_MSIX_CERTIFICATE_BASE64) {
        [IO.File]::WriteAllBytes($temporaryPfx, [Convert]::FromBase64String($env:OPENZT2_MSIX_CERTIFICATE_BASE64))
        $password = ConvertTo-SecureString $env:OPENZT2_MSIX_CERTIFICATE_PASSWORD -AsPlainText -Force
        $certificate = Import-PfxCertificate -FilePath $temporaryPfx -Password $password -CertStoreLocation Cert:\CurrentUser\My | Where-Object HasPrivateKey | Select-Object -First 1
    } elseif ($env:GITHUB_REF_TYPE -eq 'tag') {
        throw 'Tagged MSIX releases require OPENZT2_MSIX_CERTIFICATE_BASE64 and OPENZT2_MSIX_CERTIFICATE_PASSWORD.'
    } else {
        $publisher = if ($env:OPENZT2_MSIX_PUBLISHER) { $env:OPENZT2_MSIX_PUBLISHER } else { 'CN=OpenZT2' }
        $certificate = New-SelfSignedCertificate -Type Custom -Subject $publisher -KeyUsage DigitalSignature -FriendlyName 'OpenZT2 manual build' -CertStoreLocation Cert:\CurrentUser\My -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3','2.5.29.19={text}')
    }
    if (-not $certificate) { throw 'The MSIX certificate needs a private key.' }
    if ($env:OPENZT2_MSIX_PUBLISHER -and $certificate.Subject -ne $env:OPENZT2_MSIX_PUBLISHER) { throw 'The certificate subject differs from OPENZT2_MSIX_PUBLISHER.' }
    Export-Certificate -Cert $certificate -FilePath (Join-Path (Split-Path $PackagePath) 'openzt2-windows.cer') | Out-Null
    $arguments = @('sign', '/fd', 'SHA256', '/sha1', $certificate.Thumbprint, '/s', 'My')
    if ($env:GITHUB_REF_TYPE -eq 'tag') {
        $arguments += @('/tr', 'http://timestamp.digicert.com', '/td', 'SHA256')
    }
    & signtool.exe @arguments $PackagePath
    if ($LASTEXITCODE -ne 0) { throw 'MSIX signing failed.' }
} finally {
    if ($certificate) { Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -DeleteKey -Force }
    if (Test-Path $temporaryPfx) { Remove-Item -LiteralPath $temporaryPfx -Force }
}
