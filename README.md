# Perfwork

**Perfwork** is a modern, cross-platform 2D CAD designer for **perfboards**, **stripboards**, and **breadboards**, written in Rust using `egui` / `eframe`. It runs natively as a hardware-accelerated desktop application (macOS, Linux, Windows) and compiles to WebAssembly (`wasm32-unknown-unknown`) via `trunk`.

## Features

1. **Autodesk Fusion-Inspired Floating UI & Add Dialog**
   - **Standard Window Menu Bar** (`File`, `Edit`, `View`) at the top for file operations, undo/redo, and view toggles.
   - **Floating Icon Toolbar** suspended at the top-center of the workspace with Add, Select, Move, Pin, Header, Wire, Solder, Cut, Erase, Ghost, and Flip tools. Icons are painted as vector geometry, so no system fonts or emoji fonts are required.
   - **Unified Add Dialog (`A` or `Right-Click → Add Board / Component...`)**: Browse and place Perfboards/Stripboards/Breadboards, Single Pins, Parametric Headers, and Standard Library Components from one dialog.
   - **Multi-Selection & Clear All**: Press `Ctrl+A` / `Cmd+A` to select all boards, components, wires, and solder traces at once (`Del` to delete all selected), or use `Clear All` to wipe the plane completely empty.
   - **Double-click to Add**: Double-click a board, component, single-pin entry, or parametric-header entry to enter placement mode. The explicit Place buttons remain available.
   - **Move Selection (`M`)**: Select an item or group, press `M`, move the pointer, and click to place. `Esc` or right-click cancels; a completed move is one undo action. With nothing selected, `M` enables board dragging.
   - **Quick Component Copies**: `Ctrl+C` / `Cmd+C` copies selected components to the in-app component clipboard; `Ctrl+V` / `Cmd+V` attaches copies to the pointer. Click to place on free holes, or `Esc` to cancel. Copies preserve relative spacing, rotation, labels, and colors and receive new IDs and reference designators. Text fields keep their usual text-copy/paste behavior.

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
   - **Standard Component Library (`A`)**:
     - **JST-XH Shrouded Connectors (`2P`–`8P`)**: Accurately renders the 2D shrouded housing and models physical side-wall overhang that blocks 1 hole on each side of the pin row.
     - **Screw Terminals (`5.08mm` 2-Pole & 3-Pole)**, **DIP IC Sockets (`DIP-8`, `DIP-14`, `DIP-16`)**, **Axial Resistors**, **TO-220**, and **6×6mm Tactile Switches**.

5. **Wires (Straight & Spline) & Solder Traces**
   - **Wires (`W`)**: Must start and end at board holes while allowing arbitrary intermediate waypoints in between. Switch between **Straight (angled polyline)** and **Smooth Catmull-Rom Spline** routing (`Tab` while routing, or toggle per wire in the Inspector).
   - **Solder Connections (`S`)**: Drag or click across neighbor holes to create metallic solder bridges/traces. Soldered holes block new connector pins while still allowing other solder lines to connect onto them.

6. **Ghost Mode & Solder-Side View**
   - **Ghost (`G`)** makes component housings transparent enough to inspect holes and solder underneath. It is enabled automatically while routing wires, drawing solder, or cutting tracks; leaving the tool restores the manual ghost preference.
   - **Flip (`F`)** turns the workspace over into a horizontally mirrored solder-side view, keeping identifiers readable and hole coordinates unchanged. Components appear transparent and solder-side wires/traces are drawn in front. Pan, zoom, picking, and placement use the mirrored coordinates. This is a view change, not a destructive edit or a physical component flip.
   - In the flipped view, component-side wires are ghosted too (including their outlines and selection handles), while solder-side wires and through-hole pins/leads remain solid. Housing-overhang markers are hidden on the solder side; ghost wire paths do not visually block the holes they cross.

7. **Real Physical Dimensions**
   - Boards carry a millimetre outline (border, thickness, corner radius) and corner hole geometry (inset, drill, pad) alongside their hole lattice, so a board is the size it claims to be rather than a hole count with an arbitrary margin.
   - The four boards of the standard double-sided FR4 kit ship as presets: **5×7cm (24×18)**, **4×6cm (20×14)**, **3×7cm (24×10)** and **2×8cm (28×6)**. Hole counts were measured from the physical boards; all four share one border constant and reproduce their catalogue size to within 0.3mm.
   - **Edge solder pads** (the oversized slots down each side) are optional per board and are ordinary solderable holes: they join nets, accept wires and participate in DRC. They are named `L`/`R` plus the row.
   - The corner-hole **drill diameter is the one dimension not confirmed with calipers** — the reference photograph resolves ~5 px/mm. It is editable per board in the inspector, and it is the number an enclosure standoff has to match.

8. **Export for CAD**
   - **STEP (AP214)** — `File → Export STEP (3D)...` writes one solid per board and per component. Boards are prisms drilled at their mounting holes; components are bounding prisms extruded from library body heights. Enough for enclosure clearance, standoff alignment and fastener placement. The 2.54mm hole matrix is deliberately not drilled: hundreds of cylindrical faces cost real time in every CAD kernel and nothing passes through those holes from outside.
   - Component heights are nominal values for the common variant of each part. Parts sold in several heights — tactile switches especially — can be overridden per placement in the inspector.
   - **PNG front & back** — `File → Export Board Images (PNG)...` renders both sides of every board at exactly its millimetre extents. STEP carries per-face colour but has no texture concept, so artwork travels as a separate image: apply the PNG to the solid's top face at the board's own width and height and it lands 1:1 with no scaling guesswork.

## Building & Running with Nix

```bash
# Enter the development shell (includes cargo, rustc, clippy, rustfmt, trunk)
nix develop

# Run the desktop application directly via Nix
nix run

# Build the release package via Nix
nix build
```

## Checks

```bash
nix develop --command cargo test
nix develop --command cargo-clippy clippy --all-targets -- -D warnings
```

Tests cover double-click placement with simulated pointer events, keyboard commands,
move/paste validation and undo/cancel behavior, automatic ghost mode, mirrored camera
transforms, headless rendering of both sides, font-independent icon geometry, board
millimetre geometry and pad clearances, and the export coordinate frames.

### Export checks that need more than `cargo test`

Two export paths cannot be judged from inside the test suite, and both have already hidden
a real defect behind a passing build:

```bash
# Round-trip a STEP export through FreeCAD's OpenCASCADE kernel.
# Catches faces that are well-formed but fail to sew into solids.
nix develop --command ./tools/validate_step.sh

# Render every board front and back through a real GL context.
# A uniformly-coloured output means the render failed without erroring.
nix develop --command ./tools/validate_png.sh
```

`validate_step.sh` needs FreeCAD (`FREECAD=/path/to/freecadcmd` to override the default
macOS location). `validate_png.sh` opens a window briefly, since offscreen rendering needs
a GPU context that only exists inside one.
