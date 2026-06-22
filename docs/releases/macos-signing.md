# macOS Signing And Notarization

Videorc beta DMGs use the same Apple Developer signing shape as AgentPacks:

- `CSC_LINK`
- `CSC_KEY_PASSWORD`
- `APPLE_ID`
- `APPLE_APP_SPECIFIC_PASSWORD`
- `APPLE_TEAM_ID`

`CSC_LINK` should be the base64-encoded Developer ID Application `.p12` export
or another electron-builder supported certificate link. Do not commit the
certificate, password, Apple ID, or app-specific password.

## GitHub Secrets

Verify the source AgentPacks secret names:

```sh
gh secret list --repo TheOrcDev/agent-packs-desktop
```

Install the same named secrets on Videorc after exporting the unused
AgentPacks certificate material from the secure source of truth:

```sh
gh secret set CSC_LINK --repo TheOrcDev/videogre --body-file ./DeveloperIDApplication.p12.base64
gh secret set CSC_KEY_PASSWORD --repo TheOrcDev/videogre
gh secret set APPLE_ID --repo TheOrcDev/videogre
gh secret set APPLE_APP_SPECIFIC_PASSWORD --repo TheOrcDev/videogre
gh secret set APPLE_TEAM_ID --repo TheOrcDev/videogre
```

The local keychain identity currently expected for beta signing is:

```text
Developer ID Application: Uros Miric (C2PA37RB58)
```

Run the preflight before cutting a beta:

```sh
pnpm release:preflight:macos
```

The release workflow also runs this preflight before the expensive verification
and packaging steps, so missing signing or notarization secrets fail early.
