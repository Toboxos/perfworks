# Perfwork

**Perfwork** is a modern, cross-platform 2D CAD designer for **perfboards**, **stripboards**, and **breadboards**, written in Rust using `egui` / `eframe`. It runs natively as a hardware-accelerated desktop application (macOS, Linux, Windows) and compiles to WebAssembly (`wasm32-unknown-unknown`) via `trunk`.

## Features

1. **Autodesk Fusion-Inspired Floating UI & Add Dialog**
   - **Standard Window Menu Bar** (`File`, `Edit`, `View`) at the top for file operations, undo/redo, and view toggles.
   - **Floating Icon Toolbar** suspended at the top-center of the workspace with quick tool icons (`➕ Add (A)`, `↖ Select`, `✥ Board`, `⊙ Pin`, `⋮⋮ Header`, `〰 Wire`, `☍ Solder`, `⊗ Cut`, `⌫ Erase`).
   - **Unified Add Dialog (`A` or `Right-Click → Add Board / Component...`)**: Browse and place Perfboards/Stripboards/Breadboards, Single Pins, Parametric Headers, and Standard Library Components from one dialog.
   - **Multi-Selection & Clear All**: Press `Ctrl+A` / `Cmd+A` to select all boards, components, wires, and solder traces at once (`Del` to delete all selected), or use `Clear All` to wipe the plane completely empty.

2. **KiCad-Inspired Dual Control Modes (Touchpad & Mouse)**
   - Switch between **Touchpad (KiCad Pan/Pinch)** and **Mouse (Wheel Zoom / Mid-Pan)** at any time via the dropdown in the bottom-right status bar.
   - **Touchpad Mode**: Two-finger scroll pans smoothly across the plane; Pinch gesture or `Cmd`/`Ctrl` + scroll zooms centered on the cursor.
   - **Mouse Mode**: Scroll wheel zooms centered on the cursor; Middle-drag, Right-drag, or `Space` + Left-drag pans.

3. **Multi-Board Plane & Customizable Perfboards**
   - Place multiple boards of custom column × row dimensions and move them freely on the 2D plane.
   - Customizable row and column coordinate identifiers per dimension: `1, 2, 3...`, `0, 1, 2...`, `A, B, C...`, `a, b, c...`, or hidden.
   - Supports **Perfboard** (isolated pads), **Stripboard** (horizontal or vertical copper strips with interactive track cuts), and **Breadboard** (terminal strips) across 5 PCB color themes.

4. **Connector Pins, Parametric Headers & Standard Footprint Library**
   - **Single Connector Pins (`P`)**: Round or square-pad single pins.
   - **Parametric Header Lines (`H`)**: Arbitrary `N × M` male or female pin headers (`1×2` through `4×40`).
   - **Standard Component Library (`L`)**:
     - **JST-XH Shrouded Connectors (`2P`–`8P`)**: Accurately renders the 2D shrouded housing and models physical side-wall overhang that blocks 1 hole on each side of the pin row.
     - **Screw Terminals (`5.08mm` 2-Pole & 3-Pole)**, **DIP IC Sockets (`DIP-8`, `DIP-14`, `DIP-16`)**, **Axial Resistors**, **TO-220**, and **6×6mm Tactile Switches**.

5. **Wires (Straight & Spline) & Solder Traces**
   - **Wires (`W`)**: Must start and end at board holes while allowing arbitrary intermediate waypoints in between. Switch between **Straight (angled polyline)** and **Smooth Catmull-Rom Spline** routing (`Tab` while routing, or toggle per wire in the Inspector).
   - **Solder Connections (`S`)**: Drag or click across neighbor holes to create metallic solder bridges/traces. Soldered holes block new connector pins while still allowing other solder lines to connect onto them.

## Building & Running with Nix

```bash
# Enter the development shell (includes cargo, rustc, clippy, rustfmt, trunk)
nix develop

# Run the desktop application directly via Nix
nix run

# Build the release package via Nix
nix build
```
