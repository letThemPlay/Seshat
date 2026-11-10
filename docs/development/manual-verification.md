# Manual verification checklist

Everything that automated tests can't cover: real GNOME and KDE Plasma sessions, displays and scaling, drag and drop with other applications, removable devices, the real Trash and screen readers. The original specification requires GNOME and KDE verification for a release (§10.4, §10.6, §13.4).

Plan **one session per desktop** (GNOME Wayland, then KDE Plasma Wayland), about an hour each. Sections 9 (Orca) and 10.3 (a second user) only need doing on one desktop. Tick the boxes as you go, write anything unexpected in the notes, and record the results as described in [Recording results](#recording-results).

> **Safety:** every check that changes files works inside the fixture folder (`~/seshat-verify`). Two checks touch shared state: the **Trash** (section 7) and a **USB stick** (section 8). Use a stick whose contents you can lose, and don't empty the Trash unless it only holds fixture files.

## Before you start

- ✅ **Build Seshat** from the commit you are verifying:
  ```sh
  nix build                      # result/bin/seshat and result/bin/seshat-progress
  ```
  (Without Nix: `cargo build --release -p seshat -p seshat-progress`; the binaries are in `target/release/`.)
- [ ] **Create the fixtures:** `scripts/manual/verification-fixtures.sh` (makes `~/seshat-verify`; delete it to start over).
- ✅ **Start a fresh workspace,** so earlier sessions don't interfere: move `~/.local/state/seshat/workspace.toml` aside if it exists.
- [ ] **Start Seshat with a log:**
  ```sh
  RUST_LOG=info,seshat_presentation=debug result/bin/seshat 2>&1 | tee seshat-$(date +%F)-<desktop>.log
  ```
- [ ] **Note the environment:** desktop and version, distribution, GPU, monitors and their scales.

## 1. Native Wayland (spec §10.5)

- ✅ `result/bin/seshat --check-backend` prints `display backend: wayland (native, …)` and exits 0 (`echo $?`).
- ✅ Negative control: `GDK_BACKEND=x11 result/bin/seshat --check-backend` reports X11 and exits **1**.
- ✅ The log of the running app contains `display backend verified backend=wayland (native, …)`. **Copy that line into the results.**
- ✅ The compositor confirms it is a Wayland client:
  - GNOME: `Alt+F2` → `lg` → *Windows*: Seshat is not marked as an X11 window (or `xlsclients` does not list it).
  - KDE: open the KWin debug console (`qdbus6 org.kde.KWin /KWin org.kde.KWin.showDebugConsole`; `qdbus` on Plasma 5) → *Windows*: Seshat is under *Wayland Windows*.

## 2. Window and display

- ✅ Resize, maximise and restore the window: the layout follows, nothing is clipped.
- ✅ **Light and dark:** switch the desktop between light and dark (GNOME *Settings → Appearance*; KDE *Colours* or a dark Global Theme). With Seshat's theme on *System* (the default), it follows straight away.
- ✅ **Fractional scaling** at 125% and then 150% (GNOME *Settings → Displays*; if the option is missing, run `gsettings set org.gnome.mutter experimental-features "['scale-monitor-framebuffer']"` and log in again. KDE *System Settings → Display Configuration*):
  - ✅ Text and icons are sharp, not blurry.
  - ✅ The selected item in the left pane has rounded corners and the same background as the panels.
  - ✅ The branch connectors and the cards of other branches (section 3) are crisp and line up with the panel headers.
  - ✅ Focus Mode (`zz`) looks right at both scales.
- ✅ **Multiple monitors** (if available, ideally with different scales): move the window to the other monitor. It redraws at that monitor's scale, and nothing is lost.
- ✅ **Short window:** with two branches open (section 3), make the window about half as tall. The branches collapse to one row with a "+1 branch above/below" button, which opens the branch overview. Make it tall again and both rows come back.

Notes:

## 3. Workspace and keyboard

Work in `~/seshat-verify` (open it with `Ctrl+L`, type `~/seshat-verify`, `Return`).

- ✅ `j`/`k` move the cursor, `gg`/`G` go to the first and last item, and `Enter` on `Documents` opens it as a new panel to the right.
- ✅ Open `Projects` inside `Documents`, press `h` twice to get back to `seshat-verify`, then open `Downloads`. There are now **two branches**: the branch bar shows two entries, `seshat-verify` is drawn once across both rows, and connectors link the folders.
- ✅ `[` and `]` switch between the branches, and each comes back where you left it (cursor, scroll position).
- ✅ `zz` enters Focus Mode: one panel fills the space and *FOCUS* shows in the status bar. `h`, `l`, `[` and `]` still work, and `zz` restores the side-by-side layout.
- ✅ `v` and `V` select items (Visual Mode). Selecting in two different panels shows "N selected in 2 panels".
- ✅ `:` opens the command line. `:open ~/seshat-verify/Arc` + `Tab` completes to `Archive/`, and `:help` lists the keys and commands.
- ✅ Copy across branches: select `keep.txt` in `Downloads` and `notes.txt` in `Documents`, then `:copy ~/seshat-verify/Pictures`. A confirmation lists both source folders, and after confirming both files are in `Pictures`.
- ✅ Conflict: copy `Downloads/report.txt` into `Documents/Reports`. A dialog offers Replace, Skip, Keep Both and Cancel, and nothing is overwritten unless you choose Replace.
- ✅ The names `caf�.txt` (not valid UTF-8, shown with a replacement character), `file with spaces.txt` and the two links show up. The broken link does not open.
- ✅ Your **input method** (IBus or Fcitx, if you use one) does not swallow single-letter keys such as `h` and `j`.

Notes:

## 4. Mouse and touchpad

- ✅ Clicking a panel or a card of another branch makes it active, and clicking an entry in the branch bar switches branch.
- ✅ Double-clicking a folder opens it as a new panel, and double-clicking a file opens it in its application.
- ✅ Ctrl+click and Shift+click select several items. A plain click only changes the selection in that panel.
- ✅ **Touchpad:** with several panels, a two-finger horizontal swipe scrolls the workspace smoothly. Shift + mouse wheel scrolls sideways too.
- ✅ The toolbar's Focus Mode button, the Branches button, the Back/Forward buttons and the mouse's back/forward buttons work.
- ✅ Right-click on a file and on empty space opens the matching menu.

Notes:

## 5. Clipboard and drag and drop

Use the desktop's file manager (GNOME *Files*, KDE *Dolphin*) next to Seshat.

- ✅ **Seshat → file manager:** copy `keep.txt` in Seshat (`Ctrl+C` or `y`), then paste in the file manager: the file is copied.
- ✅ **File manager → Seshat:** copy a fixture file in the file manager, then paste into a Seshat panel (`Ctrl+V` or `p`): it is copied there.
- ✅ **Cut:** cut a fixture file in one application and paste it in the other: it is **moved** in both directions.
- ✅ `Ctrl+Shift+C` on a file, then paste into a text editor: the full path appears.
- ✅ **Drag out:** drag a file from Seshat onto the file manager's window: it is copied there.
- ✅ **Drag in:** drag a file from the file manager onto a Seshat panel: it is copied (hold Shift to move).
- ✅ **Between Seshat panels:** drag a file from one panel to a panel of the other branch: it is **moved** (hold Ctrl to copy). Dropping it on a folder row puts it inside that folder.

Notes:

## 6. Opening files and portals

- ✅ `Enter` on `notes.txt` opens the desktop's default text editor.
- ✅ The toolbar's theme menu → *Open Themes Folder* opens `~/.config/seshat/themes` as a branch.

## 7. Trash (the real session Trash)

- ✅ Select `Downloads/trash-me.txt` and press `Delete`: it disappears from the folder.
- ✅ Open *Trash* in the left pane: `trash-me.txt` is listed with its original folder and the deletion date.
- ✅ Select it and choose *Restore*: it is back in `Downloads`.
- ✅ Trash `Documents/trash-me-too.txt` again, then in the Trash use *Delete Permanently…*: a confirmation appears with Cancel as the default, and confirming removes it.
- ✅ Do **not** use *Empty Trash* unless the Trash only holds fixture files.

Notes:

## 8. Removable devices (a USB stick you can wipe)

- ✅ Plug it in: it appears in the left pane under the standard locations.
- ✅ Click it: it mounts and opens as a new branch. Open a folder on it.
- ✅ Copy `large.bin` to it: the progress bar fills, and `seshat-progress` (section 10) shows the same operation.
- ✅ Copy `large.bin` again and **cancel** it from the operations panel: it stops, and no half-written `.seshat-part-*` file is left behind.
- ✅ Trash a file on the stick: it is either moved to the stick's Trash, or Seshat says it couldn't be and offers permanent deletion. It is never deleted silently.
- ✅ Use the **eject** button in the left pane. The stick's panels stay but say the device was removed, "can be removed safely" appears, and other branches are unaffected.
- ✅ Plug it in again, open it, then **pull it out while viewing it**. The panels are marked unavailable, there's no crash, and other branches are fine.
- ✅ Plug it back in, click it in the left pane to mount it, then *Retry* on the unavailable panel: its content comes back.
- ✅ Known limitation: an **encrypted** volume reports an error instead of asking for the passphrase.

Notes:

## 9. Accessibility with Orca (one desktop is enough)

Start Orca (GNOME: `Super+Alt+S`; KDE: install `orca` and run it).

- [ ] Moving through the left pane reads each place's name.
- [ ] The file list is announced as "Files", and moving with the arrow keys reads each item.
- [ ] A panel is announced as "Folder panel". The branch bar is announced as "Branches", and its entries by name and number.
- [ ] A card of another branch reads like "Branch 2, Downloads".
- [ ] The mode indicator ("Interaction mode") and the command line ("Command") are announced.
- [ ] Dialogs (rename, conflicts, confirmations, help) read their text and button labels.
- [ ] Everything in sections 3 and 7 can be done with the keyboard alone.

Notes:

## 10. Progress for other applications (D-Bus)

- ✅ In a terminal, run `result/bin/seshat-progress`, then copy `large.bin` in Seshat. The client shows its progress and completion.
- ✅ Quit and restart Seshat while the client runs: it reports that Seshat stopped, then picks up again.
- ✅ **Another user** (once, needs a second account `other`): running
  ```sh
  sudo -u other busctl --address=unix:path=/run/user/$(id -u)/bus call \
      dev.kdmorris.Seshat.Operations /dev/kdmorris/Seshat/Operations \
      dev.kdmorris.Seshat.Operations GetActiveOperations
  ```
  is **refused**. Either the bus denies the connection, or Seshat answers `AccessDenied`. Both count as a pass.

## 11. Finish

- ✅ Quit Seshat and start it again: the workspace (branches, panels, positions) is restored.
- ✅ Search the log for problems: `grep -E "panicked|ERROR|main loop stalled" seshat-*.log` (expect nothing).
- ✅ Clean up: delete `~/seshat-verify`, put your own `workspace.toml` back if you moved it aside, and remove the fixture files from the Trash and the USB stick.

## Recording results

For each desktop, fill in its column in the matrix in [wayland-verification.md](wayland-verification.md):
- ✅ with the date for a pass, ❌ for a failure (with a short note).
- Copy in the backend line from section 1.

Then update the "Not yet verified" list in [definition-of-done.md](../definition-of-done.md): remove what passed, and keep anything that failed, with a link to the issue.

For a failure, note the steps, what you expected and what happened, and keep the log and a screenshot. A short summary per desktop is enough:

| Desktop | Version | Scale(s) | Monitors | Backend line | Sections passed | Failures |
|---|---|---|---|---|---|---|
| GNOME Wayland | | | | | | |
| KDE Plasma Wayland | | | | | | |
