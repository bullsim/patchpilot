# Code signing

Unsigned builds work, but Windows shows "Unknown publisher" on the UAC prompt and
SmartScreen warns on first download, and macOS Gatekeeper refuses to open the app
without a right-click. Signing removes all of that and makes in-app updates
seamless. The CI pipeline is already wired for it: add the secrets below and
every release from then on is signed automatically. With no secrets set, builds
are simply unsigned (the sign hook no-ops), so forks and local builds keep working.

## Windows — Azure Trusted Signing (recommended)

Microsoft's own signing service. ~US$10/month, no hardware token, certificates
rotate automatically, and SmartScreen trusts them quickly. Requires an Azure
subscription and an identity check.

### 1. Identity validation (one-off, a few days)

* Azure Portal → create a **Trusted Signing account** (region `West Europe` →
  endpoint `https://weu.codesigning.azure.net`, or `East US` → `https://eus.codesigning.azure.net`).
  SKU: **Basic**.
* In the account → **Identity validation** → **Public** → choose
  **Organization** (needs a registered business, e.g. a Companies House entry,
  3+ years old) or **Individual** where offered. Fill in the details; Microsoft
  verifies them (typically 1–5 working days).

### 2. Certificate profile

* In the account → **Certificate profiles** → **Create** → type **Public Trust**,
  pick the validated identity, name it e.g. `patchpilot`. The publisher name on
  the certificate is the validated identity.

### 3. Service principal for GitHub Actions

* Entra ID → **App registrations** → **New registration** (e.g. `patchpilot-ci`).
  Note the **Application (client) ID** and **Directory (tenant) ID**.
* **Certificates & secrets** → new client secret → copy the **value**.
* Back on the Trusted Signing account → **Access control (IAM)** → **Add role
  assignment** → role **Trusted Signing Certificate Profile Signer** → assign to
  `patchpilot-ci`.

### 4. GitHub secrets

Repo → Settings → Secrets and variables → Actions → **New repository secret**:

| Secret                     | Value                                             |
|----------------------------|---------------------------------------------------|
| `AZURE_TENANT_ID`          | Directory (tenant) ID                             |
| `AZURE_CLIENT_ID`          | Application (client) ID                           |
| `AZURE_CLIENT_SECRET`      | the client secret value                           |
| `TRUSTED_SIGNING_ENDPOINT` | e.g. `https://weu.codesigning.azure.net`          |
| `TRUSTED_SIGNING_ACCOUNT`  | Trusted Signing account name                      |
| `TRUSTED_SIGNING_PROFILE`  | certificate profile name, e.g. `patchpilot`       |

Tag the next release. The Windows jobs fetch `Microsoft.Trusted.Signing.Client`
and `src-tauri/sign-windows.ps1` signs `patchpilot.exe`, the NSIS installer and
the MSI during bundling, so the updater's `.sig` covers the signed installer.
Check the run log for `sign-windows: signed OK`.

### How it is wired

* `src-tauri/tauri.conf.json` → `bundle.windows.signCommand` calls
  `sign-windows.ps1 <file>` for each binary Tauri produces.
* `sign-windows.ps1` runs `signtool sign … /dlib Azure.CodeSigning.Dlib.dll`
  with an RFC 3161 timestamp, then `signtool verify`. If any of the variables
  above is missing it prints a notice and exits 0 (unsigned).
* `.github/workflows/release.yml` → step **Prepare Azure Trusted Signing**
  (Windows only, skipped when `AZURE_CLIENT_SECRET` is unset) installs the dlib
  and exports `TRUSTED_SIGNING_DLIB`; the build step passes the secrets through.

### Alternatives

* **SignPath Foundation** — free OV signing for open-source projects
  (https://signpath.org/). Needs an OSI licence file in the repo (this repo
  currently has none) and an application review. Signing happens on their
  servers after the build, so the Tauri updater signature would have to be
  re-generated afterwards (`tauri signer sign`) — more CI plumbing than above.
* **Traditional OV/EV certificate** (Sectigo, DigiCert, SSL.com, Certum):
  US$200–600/year, must live on a hardware token or cloud HSM, so CI signing
  needs the vendor's cloud-signing service. EV gets instant SmartScreen
  reputation; OV builds it over a few weeks of downloads. Trusted Signing is
  cheaper and simpler for a single app.
* **Self-signed** certificates do not help: Windows treats them as unknown.

## macOS — Apple Developer Program (not yet wired)

Gatekeeper needs the app signed with a **Developer ID Application** certificate
and **notarized**. That requires an Apple Developer Program membership
(US$99/year). Tauri supports it natively via these secrets on the macOS job:

`APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific password),
`APPLE_TEAM_ID`.

Once you have the membership, add those secrets and pass them through in the
macOS build step's `env:`; tauri-action signs and notarizes automatically.

## Linux

No signing needed for `.deb`/`.AppImage`/`.rpm`. The updater verifies every
package with the Tauri minisign key regardless of platform.
