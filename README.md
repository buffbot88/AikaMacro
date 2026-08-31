# AikaTK Macro

A Windows Rust GUI for configuring up to eight optional keyboard-event slots with independent timers.

## Build

```bash
cargo check
cargo run --release
```

## How It Works

Each slot sends configured keystrokes using Windows `SendInput` API. The app includes a `UIAccess="true"` manifest — the same mechanism used by Windows On-Screen Keyboard (OSK.exe) — which allows it to send input to windows at higher integrity levels.

### Important: UIAccess Setup

For `UIAccess="true"` to work, the executable must be:

1. **Digitally signed** with a trusted certificate, OR
2. **Installed in `Program Files`** (not a user directory)

If UIAccess is not active, the macro falls back to standard `SendInput` behavior.

## Configuration

Each slot can be enabled or disabled and contains:

- `key` — a keystroke such as `R`, `1`, `Ctrl+1`, `Alt+Q`, `Shift+F1`, `Space`, or `Enter`.
- `interval_ms` — delay between completed activations; minimum `100` ms.
- `press_ms` — how long the key is held; minimum `1` ms.
- `release_ms` — delay after releasing the key before the interval begins; minimum `1` ms.

The eight slots run independently, so one slot's timer does not block another. The complete profile is saved to or loaded from `config.json` beside the executable.

Example:

```json
{
  "skills": [
    { "enabled": true, "key": "1", "interval_ms": 5000, "press_ms": 50, "release_ms": 50 },
    { "enabled": true, "key": "Ctrl+R", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "3", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "4", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "5", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "6", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "7", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 },
    { "enabled": false, "key": "8", "interval_ms": 1000, "press_ms": 50, "release_ms": 50 }
  ]
}
```

## Controls

- `Ctrl+P` — start globally
- `Ctrl+S` — stop globally
- The GUI also provides Start, Stop, Save, and Load buttons.

## Limitations

- The program sends documented Windows `SendInput` keyboard events. It does not load DLLs, access game memory, capture screens, inspect game state, install drivers, use AI, or bypass anti-cheat software.
- If AIKATK/nProtect blocks synthetic input, ensure the executable is properly signed or installed in `Program Files` for UIAccess to be active.
- nProtect may still detect or block automation; use it only where the game/server rules explicitly permit macros.

## Release Signing (SignPath, Open Source)

The release executable ships with a `UIAccess="true"` manifest (same mechanism as Windows On-Screen Keyboard). Windows only accepts UIAccess for executables that are **digitally signed with a trusted certificate chain** — running as administrator is not sufficient, which is why local admin launches still fail with error 740.

This repo ships a GitHub Actions workflow (`.github/workflows/release.yml`) that builds the exe on a GitHub-hosted Windows runner and submits it to [SignPath](https://signpath.io) for Authenticode signing under their free Open Source Code Signing program.

### One-time setup

1. Apply for SignPath Open Source Code Signing at <https://signpath.io>, linking your GitHub repository.
2. In SignPath, create:
   - A **project** with slug `aikatk-macro`
   - An **artifact configuration** with slug `aikatk-macro-exe` (Authenticode, targets the single `.exe` file inside the uploaded artifact)
   - A **signing policy** with slug `release-signing`
   - A **CI user** and an API token for it
3. In your GitHub repo, add repository secrets:
   - `SIGNPATH_API_TOKEN` — the CI user's API token
   - `SIGNPATH_ORGANIZATION_ID` — your SignPath organization ID

### Release flow

1. Tag a release: `git tag v0.1.0 && git push origin v0.1.0` (or run the workflow manually via **Run workflow**).
2. The workflow builds `target/release/aikatk_macro.exe`, uploads it as the unsigned workflow artifact, submits it to SignPath, waits for the signed result, then publishes the signed `AikaTK-Macro.exe` as a workflow artifact and attaches it to the GitHub Release.
3. Verify: Properties → **Digital Signatures** tab on `AikaTK-Macro.exe` shows the expected publisher/certificate.
4. Distribute **only the signed exe**, never the unsigned workflow artifact.

Note: publishing the signed exe requires the repository to allow the default GITHUB_TOKEN to create releases (standard setting, enabled by default).
