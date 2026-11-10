# Manual verification, 2026-11-10: Hyprland and Niri

A run of the [manual verification checklist](../manual-verification.md) on two Wayland compositors, by the maintainer.

| | |
|---|---|
| Desktops | Hyprland, Niri (Wayland) |
| Versions, GPU, monitors | Not recorded |
| Backend log line | Not kept. `seshat --check-backend` reported native Wayland and exited 0, and the X11 negative control exited 1 |
| Commit | `acdd047` (Add a manual verification checklist) |

## Results

| Section | Result |
|---|---|
| Before you start | ✅ (build, fresh workspace, fixtures) |
| 1. Native Wayland | ✅ |
| 2. Window and display (light/dark, 125%/150% scaling, multiple monitors, short window) | ✅ |
| 3. Workspace and keyboard | ✅ |
| 4. Mouse and touchpad | ✅ |
| 5. Clipboard and drag and drop | ✅ |
| 6. Opening files and portals | ✅ |
| 7. Trash | ✅ |
| 8. Removable devices | ✅ (the encrypted-volume limitation behaves as documented) |
| 9. Accessibility with Orca | ☐ Not done |
| 10. D-Bus, including a request from another user | ✅ |
| 11. Finish (restore after restart, log check, clean-up) | ✅ |

No failures were reported.

## Still to do

- GNOME and KDE Plasma Wayland sessions (the original specification's target desktops).
- Orca (section 9).
