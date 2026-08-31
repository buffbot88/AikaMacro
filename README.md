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
