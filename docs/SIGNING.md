# Publisher Signatures and Verification

Numera Emoji Mapper publishes signed standalone binaries and release archives under the pinned
publisher identity **KiaroSama** (`CN=KiaroSama, OU=Numera Emoji Mapper`), using a dedicated
RSA-3072 code-signing certificate.

Public verification material lives in [`docs/signing/publisher.json`](signing/publisher.json):

- **Publisher:** `KiaroSama`
- **Product:** `Numera Emoji Mapper`
- **Algorithm:** `rsa-pkcs1-sha256` (RSA-3072)
- **Certificate SHA-1 thumbprint:** `BC5B86D9D64A2F127A59A6BE6AF7CB39C2332ABB`
- **Certificate SHA-256:** `2712e415ca2ea530cc216bae4c0e486d594c109803e5dcce8bd74bc00a38e69c`
- **Public key SHA-256:** `8fb8cabc787bd1f3efe6a2141fcd201ad0805fa61e801bf8d64c21da722f3931`

This is a self-signed publisher identity, not a commercial CA certificate. Verify downloads against
the pinned thumbprint and public key in `docs/signing/publisher.json`, or against the
`publisher-cert.pem` and `publisher-key.pem` files attached to each GitHub release.

## How releases are signed

Hosted releases run inside the protected GitHub Actions environment `numera-signing` across all
four 64-bit targets (`windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`):

1. Each target builds `numera-emoji`, verifies the embedded project logo (`numera-emoji verify-binary`),
   and passes the offline package integrity suite (`standalone_package`).
2. Windows targets sign `numera-emoji.exe` with Microsoft Authenticode (`signtool.exe`, SHA-256)
   and an RFC 3161 DigiCert timestamp before generating the final archive so `package.json` records
   the digest of the signed executable.
3. Every target signs its release archive with RSA-3072 SHA-256 (`<archive>.zip.sig`), verifies the
   signature against the pinned public key, and confirms that a one-byte tampered copy fails
   verification.
4. The release publisher signs `SHA256SUMS` (`SHA256SUMS.sig`), emits GitHub build provenance
   attestations (`actions/attest-build-provenance`), and uploads the archives, `.sha256` sidecars,
   `.sig` detached signatures, `SHA256SUMS`, `SHA256SUMS.sig`, `publisher-cert.pem`, and
   `publisher-key.pem`.

## Verifying a downloaded release

### 1. Verify `SHA256SUMS` and archive digests

```bash
openssl dgst -sha256 -verify publisher-key.pem -signature SHA256SUMS.sig SHA256SUMS
sha256sum -c SHA256SUMS
```

### 2. Verify an individual archive signature

```bash
openssl dgst -sha256 -verify publisher-key.pem \
  -signature numera-emoji-mapper-3.4.0-linux-x64.zip.sig \
  numera-emoji-mapper-3.4.0-linux-x64.zip
```

### 3. Verify the Windows Authenticode signature

```powershell
$sig = Get-AuthenticodeSignature .\numera-emoji.exe
$sig.SignerCertificate.Thumbprint -eq 'BC5B86D9D64A2F127A59A6BE6AF7CB39C2332ABB'
$null -ne $sig.TimeStamperCertificate
```

### 4. Verify GitHub build provenance

```bash
gh attestation verify numera-emoji-mapper-3.4.0-windows-x64.zip --owner KiaroSama
```
