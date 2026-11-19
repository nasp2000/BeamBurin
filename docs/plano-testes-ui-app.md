# UI and menu test plan (desktop app)

Status: planning document for the desktop UI round after version 0.1.0.
Owner: BeamBurin maintainers. Version under test: 0.1.0 (build from
`~/Code/Apps/BeamBurin`, 19 November 2026, commit reported by Help > About).

Nothing is implemented in this phase. This document is the deliverable.

## 1. Purpose

The automated suites exercise the document model, the exporters, the MCP server
and a growing set of `egui_kittest` screens. They do not yet prove that every
menu path, dialog field, panel flow and error state behaves as described for a
person at the keyboard.

This plan closes that gap with numbered cases grouped by UI area. Each case has
an objective, concrete steps, the expected result and the driver that can prove
it. Cases that need a real desktop session, the system clipboard, a browser or
another native integration are marked **MANUAL** and listed again in section 9.

## 2. Method and environment

* **Build:** installed Linux aarch64 binary `~/Code/Apps/BeamBurin`, version
  0.1.0. The About dialog shows the version, profile, target and commit.
* **Exploration pass:** the installed binary was queried through its MCP server
  (`BeamBurin mcp`) with `initialize`, `tools/list` and `list_commands`. That
  returned **108 agent tools** and **178 catalogued commands** (71 application
  commands plus agent tools). The menu and dialog inventory in this plan was
  then audited against `src/app/chrome.rs`, `src/app/*.rs` and
  `src/commands.rs`.
* **Drivers:**
  * `kittest`: the in-repo `egui_kittest` harness used by `src/app/tests.rs`.
    It drives the accessibility tree (labels, roles, clicks, keys) headlessly
    in CI.
  * `mcp`: the installed binary's `BeamBurin mcp` server for document and file
    assertions (`get_status`, `inspect`, `list_objects`, `measure`,
    `check_job`, exports).
  * **MANUAL**: a desktop session with a real window manager, browser,
    clipboard, printer/viewer or operating-system integration.
* **Voice proof:** none. The interface is checked by semantic tree, files and
  manual navigation, never by dictation.
* **Screenshots:** not required for pass/fail; any visual judgement (contrast,
  icon shape, dark mode) is MANUAL and recorded with a screenshot.
* **Evidence:** per run, the kittest output, the MCP JSON calls and results, the
  files produced, and the verdict. One record per area, stored outside the
  public tree.
* **Fixtures:** section 4. All synthetic; no private customer files.
* **Pass rule:** a case passes only when the expected result is observed. A
  dialog that opens but whose fields do not reach the document or file is a
  failure, not a pass.

### 2.1 Conventions

* Menu paths use the labels exactly as shown, including `...`.
* `click "Run"` means click the visible button with that label in the focused
  window. `type` means replace the field value.
* Every new dialog is first inspected for labelled widgets and disabled states;
  a case that only opens a window is not enough.
* `Esc` closes the focused dialog unless the case says otherwise. The case then
  checks that the document, selection and undo history did not change.
* Assertions use the status bar text, the window title, the `inspect` output,
  the file written, or the visible values after reopening the dialog.
* The in-app file browser is used by BeamBurin; the native picker is not part
  of the flow. Cases that still need the operating system are marked MANUAL.

## 3. Baseline and gap analysis

### 3.1 What the current suites prove

* 138 `egui_kittest` tests in `src/app/tests.rs`. They cover: top-level menus
  opening with one entry, the object and canvas context menus, the toolbar and
  align actions, several Arrange/Edit flows, cuts/layers rows, the preview and
  G-code export, SVG/DXF/PDF/PNG export dialogs, presets, image adjustment,
  licences, About, shortcuts, preferences sections and the command palette.
* Rust unit and integration tests for geometry, booleans, offsets, arrays,
  warps, tracing, presets, G-code, print/PDF, save/load and the MCP server.
* CI runs on every push: format, clippy, doctests, MSRV, deny, corpus, headless
  UI tests, Xvfb smoke, Linux aarch64 and Windows x86_64 builds.

### 3.2 What is still untested

| Gap | Size in this build | Planned cases |
|---|---|---|
| File and project lifecycle, dirty guards, recent projects, recovery | no end-to-end round | UI-F |
| Import paths: SVG/DXF options, raster limits, relink, collect, malformed files | partial | UI-I |
| Every vector exporter option, margins outside the page, selection-only exports | partial | UI-V |
| Raster export options and PNG edge cases | partial | UI-R |
| G-code preview, estimate, material test, registration marks and the print PDF | partial | UI-G |
| Clipboard, paste in place, selection filters, locking and groups | partial | UI-E |
| Draw tool edge behaviour (Shift, exact size, Bézier, freehand, measure) | partial | UI-D |
| Canvas navigation, guides, snapping and node editing | partial | UI-N |
| Arrange/transform submenus, docking, nesting, repeated copies | partial | UI-T |
| Boolean/path operations and their error states | partial | UI-B |
| Panels, layer operations, colour palette and property editors | partial | UI-P |
| Cuts/operations editors, parameters, constraints, recipes, machines | partial | UI-O |
| Preferences search/reset, workspaces, interface presets, shortcuts editing | partial | UI-S |
| Palette ranking, disabled commands, help topics and the tour | partial | UI-Q |
| Agent/MCP-only actions (dock, print, trace, Shape Maker, presets) | not in the UI | UI-A |
| Error paths, disabled states, accessibility labels, persistence | scattered | UI-Z |

### 3.3 Coverage rule

A menu entry, dialog or panel is covered when at least one case exercises its
main action and one case checks its invalid/disabled state. Every top-level
menu and every window listed in section 5 is mapped to at least one case. The
MCP-only actions are covered in UI-A because they have no menu path yet.

## 4. Fixtures

| ID | Shape | Purpose |
|---|---|---|
| F1 | `ui-base.bwp`: A4 landscape, two layers, rectangle, ellipse, star, text, open path, closed path with a hole | most edit/arrange/export cases |
| F2 | `ui-import.svg`: nested groups, text, stroke widths, a malformed variant | import and error paths |
| F3 | `ui-import.dxf`: line/arc/polyline entities, unsupported entity | DXF import |
| F4 | `ui-image.png` (RGBA), `ui-photo.jpg`, `ui-alpha.webp` | image import, mask, crop, export |
| F5 | `ui-large.bwp`: 2 000 objects, 40 layers | performance, panels, cancellation |
| F6 | `ui-job.nc`: external G-code with comments and arcs | external preview |
| F7 | `ui-presets-image.bwp-presets`, `...-vector...`, `...-gcode...`, plus empty, wrong-kind, unknown-field, >1 MiB variants | preset files |
| F8 | `ui-barcode.txt`: short, long and non-encodable strings | barcode |
| F9 | `ui-font.txt`: accented characters, uppercase, multi-line | text editor and outlines |
| F10 | `ui-cuts.bwp`: fill and line operations, disabled registration layer, tabs | G-code and print |
| F11 | `ui-constraints.bwp`: two paths with a coincident and a distance constraint | constraints |
| F12 | empty new project | empty-state cases |

Fixture files live with the round outside the public tree; only F1 and F8 are
small enough to keep in a private validation folder.

## 5. Cases

The plan has 196 cases in 16 areas. Twenty-one are MANUAL only (system
clipboard, browser handoff, visual theme/contrast, window manager behaviour,
real printer/viewer, crash recovery, screen reader, OS-specific paths). The
rest run headlessly with `kittest` and `mcp`.

### 5.1 File and projects (UI-F)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-F01 | New project starts empty | `File > New` on a clean app | A4 portrait page, one layer, no objects, title "Untitled", status "New document" | kittest |
| UI-F02 | New project asks before discarding changes | edit F1, `File > New` | `Unsaved changes` dialog with Save/Discard/Cancel; Cancel keeps everything | kittest |
| UI-F03 | Open project through the in-app browser | `File > Open...`, pick F1 | objects, layers, operations and page size match; status names the path; fit page runs | kittest |
| UI-F04 | Open a drawing imports into a project | `File > Open...`, pick F2 | SVG entities become objects, unsupported features warn, document is valid | kittest + MCP `inspect` |
| UI-F05 | Save writes the current path and clears dirty | edit F1, `File > Save` | no dialog, status "Saved", title loses the dirty marker, file mtime changes | kittest |
| UI-F06 | Save as writes a new path | `File > Save as...`, new name | new file exists, old file unchanged, title shows the new name | kittest |
| UI-F07 | Save project with images embeds assets | open F1 with F4, `File > Save project with images...` | `.bwpz` contains the images; reopening works without the original folder | kittest + MCP |
| UI-F08 | Recent projects list reopens a file | after UI-F06, `File > Recent projects` | the recent entry is present and opens the same project | kittest |
| UI-F09 | Recover project uses the backup | corrupt the `.bak`, `File > Recover project...` | the recovered document matches the last good state and status says what happened | kittest |
| UI-F10 | Collect images copies assets beside the project | `File > Collect images...` with F4 | images copied, object paths rewritten, missing image reported, no data loss | kittest + MCP |
| UI-F11 | Document properties edits page and metadata | `File > Document properties...`, change name/size/unit, OK | page redraws, rulers/status update, one undo step, Cancel changes nothing | kittest |
| UI-F12 | Exit with unsaved changes can be cancelled | edit, `File > Exit` | confirmation dialog; Cancel keeps the window and document | MANUAL (window close) |

### 5.2 Import and assets (UI-I)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-I01 | Import SVG through the menu | `File > Import SVG or DXF...`, F2 | objects appear, layer colour and stroke kept, warnings for unsupported items | kittest |
| UI-I02 | Import DXF keeps units | import F3 | geometry matches the header unit, unsupported entities reported, no crash | kittest |
| UI-I03 | Import rejects an unknown extension | pick a `.txt` file | file browser refuses or the import reports "unsupported"; document unchanged | kittest |
| UI-I04 | Import image honours the pixel limit | import an image above the configured limit | error names the limit, no partial object, status explains how to raise it | kittest |
| UI-I05 | Import image with alpha keeps transparency | import F4 RGBA, open Properties | bounds and DPI correct, preview shows the alpha, no white box | kittest |
| UI-I06 | Relink a missing image | move the image file, use Properties > Relink image, pick it | object preview returns, path updated, status names the file | kittest |
| UI-I07 | Collect/relink across a moved project | move a `.bwp` and its folder together | paths resolve relative to the project; no broken preview | kittest |
| UI-I08 | Preview external G-code | `File > Preview external G-code...`, F6 | preview window opens, layers/paths listed, playback controls work, Cancel closes | kittest |
| UI-I09 | Import malformed SVG reports without crashing | import the malformed F2 variant | error on the status bar, previous document intact, undo history unchanged | kittest |
| UI-I10 | Drag and drop a supported file | drag F1 onto the window | confirmation or direct open, same result as `File > Open` | MANUAL |

### 5.3 Vector export (UI-V)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-V01 | Export SVG whole page | `File > Export SVG...`, Save | file exists, page size in `mm`, all visible layers, expected objects | kittest + file |
| UI-V02 | SVG options: text as paths and embedded images | open the SVG dialog, tick both, export | `<text>` absent, paths present, image data URI present, warning lists omissions otherwise | kittest |
| UI-V03 | SVG layer filter | export only layer A | viewBox and contents include only layer A; hidden layers absent | kittest |
| UI-V04 | SVG selection export | select two objects, `File > Export selection > As SVG...` | page equals selection bounds plus margin; other objects absent | kittest |
| UI-V05 | Margin outside the page | selection export, margin 10 mm, "Allow margin outside the page" | viewBox grows past the page, content keeps its offset, flag is saved in the preset | kittest |
| UI-V06 | DXF whole page and units | `File > Export DXF...`, mm and inch | header `$INSUNITS` matches, coordinates scale correctly | kittest |
| UI-V07 | DXF arcs option | export a circle with arcs on and off | arcs become `ARC`/bulges when on, polylines when off | kittest |
| UI-V08 | PDF whole page and selection | export both | one page, MediaBox matches paper/selection, text as paths when chosen | kittest |
| UI-V09 | EPS bounding box | export F1 | `%%BoundingBox` matches the region in points, content shifted by the origin | kittest |
| UI-V10 | HPGL origin | export a selection as HPGL | first move starts near the plot origin, pen per layer cycles, negative coordinates allowed | kittest |
| UI-V11 | Empty/invalid area is refused | set a zero or outside selection and export | dialog stays open, status explains the area, no file written | kittest |
| UI-V12 | Export presets apply and save all vector options | manage presets, save current DXF options, change them, apply | format, arcs, unit, margin, framing, layers and outside-page flag restored; summary reflects them | kittest |

### 5.4 Raster export (UI-R)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-R01 | PNG whole page at a DPI preset | `File > Export image...`, 300 dpi | pixel size matches page at 300 dpi, white background | kittest + file |
| UI-R02 | Exact DPI shows the custom field | choose "Custom..." | Exact DPI field appears, value clamps 10–2400 | kittest |
| UI-R03 | Transparent background | tick Transparent, export | alpha channel present, corners transparent | kittest |
| UI-R04 | Custom pixel size keeps aspect | set width, keep aspect on | height follows the page ratio; off stretches with a warning | kittest |
| UI-R05 | Margin and offsets | set margin and X/Y offsets | frame grows/shifts as shown, preview size updates, exported pixels match | kittest |
| UI-R06 | Page / drawing / selection framing | export the three modes | page includes everything, drawing crops to visible bounds, selection crops to the selection | kittest |
| UI-R07 | Selection with margin outside the page | tick the checkbox | canvas origin can be negative, image size grows, no clipping of the frame | kittest |
| UI-R08 | Image export presets restore every field | save a preset, change fields, apply | dpi, custom DPI, transparency, margin, offsets, size, aspect, framing and outside-page flag restored | kittest |
| UI-R09 | Huge output is refused | 600 dpi on a very large page | error names the pixel limit, no file, dialog stays open | kittest |
| UI-R10 | Export image of text and images | F1 plus F4 | text rasterises with its style, image keeps colours/alpha, no missing object | kittest |

### 5.5 G-code, preview and print (UI-G)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-G01 | Cuts/Layers edits an operation | open `Operations > Cuts / Layers...`, change speed/power/passes | values persist, object rows show the new summary, one undo step | kittest |
| UI-G02 | Operation output options reach the job | toggle output, comments, origin, arcs | G-code reflects each option; `check_job` agrees | kittest + MCP |
| UI-G03 | Layer colour drives the default operation | add a layer with a new colour | a matching operation is created/bound, objects use it | kittest |
| UI-G04 | Preview and G-code opens with playback | `Operations > Preview and G-code...` | preview window, canvas toolpaths, line count, play/pause/speed work | kittest |
| UI-G05 | Preview catches a job error | disable a layer or use an empty document | warnings/errors listed with codes, export disabled where appropriate | kittest |
| UI-G06 | Export G-code writes the file | from the preview or File menu | file exists, header/footer, `check_job` clean, status names path | kittest + file |
| UI-G07 | Estimate duration is consistent | `Operations > Estimate duration` | status shows paths and seconds; re-estimate after moving shapes changes it | kittest + MCP `estimate` |
| UI-G08 | Material test dialog builds a grid | `Operations > Material test...`, set speed/power/passes, OK | preview grid, confirm creates the objects and one undo step | kittest |
| UI-G09 | Registration marks dialog | `Operations > Registration marks...`, four marks, apply | marks in their layer, visible, outside the job, one undo step | kittest |
| UI-G10 | Print PDF (MCP-only surface) | `mcp: export_print` black and colour, A4 | one-page PDF, correct MediaBox, black/colour raster, no document change | MCP |
| UI-G11 | External G-code preview is read-only | open F6, change playback | no document object created, export controls absent or disabled | kittest |
| UI-G12 | Job cancellation | start a large export, cancel | status "Cancelled", no partial file, app responsive | MANUAL (timing) |

### 5.6 Edit, clipboard and selection (UI-E)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-E01 | Undo/redo labels and history | make three edits, undo twice, redo once | titles name the action, document matches, redo disabled at the end | kittest |
| UI-E02 | Cut and paste offsets by 5/−5 | select F1 object, Cut, Paste | new id, name "copy", group cleared, one undo per action | kittest |
| UI-E03 | Paste in place (MCP-only surface) | copy, `mcp: paste_in_place` | same coordinates, hole links remapped, one undo | MCP |
| UI-E04 | Copy/paste a hole with its parent | select outer and hole, copy, paste | new hole points to the new outer, no orphan | kittest + MCP |
| UI-E05 | Copy/paste a hole without its parent | copy only the hole, paste | link cleared with a warning in status | kittest |
| UI-E06 | Duplicate keeps style and offset | select text and image, Duplicate | same style/layer/operation, 5/−5 offset, selection is the copy | kittest |
| UI-E07 | Delete and undo | delete a mixed selection | objects removed, selection empty, undo restores ids and order | kittest |
| UI-E08 | Select all/none/invert | with hidden and locked objects | each command follows visibility/lock rules; status counts match | kittest |
| UI-E09 | Select open shapes and open-fill shapes | F1 plus an open path with fill | only open paths (with fill for the second), closed shapes excluded | kittest |
| UI-E10 | Smaller-than-selected selection | one 30 mm reference, smaller/larger/equal shapes | only strictly smaller visible/unlocked shapes selected; multiple references error | kittest |
| UI-E11 | Select all on layer / contained | mixed layers, nested shapes | correct subset, container stays selected for contained | kittest |
| UI-E12 | Group/ungroup | two objects, Group, move, Ungroup | one unit moves, ungroup keeps positions, undo restores grouping | kittest |
| UI-E13 | Lock prevents editing | lock a layer/object, try move/delete/dock | actions disabled or report locked, unlock restores | kittest |
| UI-E14 | System clipboard image/SVG/text | copy an image in another app, `Ctrl+V` | image imported, SVG pasted as objects, text becomes a text object | MANUAL |

### 5.7 Draw tools (UI-D)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-D01 | Rectangle drag and Shift | drag, then Shift-drag | bounds match drag; Shift makes a square | kittest |
| UI-D02 | Exact size rectangle/ellipse | tick Exact size, 40×20 mm, drag | object is exactly 40×20 regardless of drag | kittest |
| UI-D03 | Ellipse and circle | drag, Shift-drag | correct radii and bounds | kittest |
| UI-D04 | Polygon sides | set 6 sides, click-drag | six vertices, regular shape, radius follows drag | kittest |
| UI-D05 | Star tips and inner ratio | set tips/inner, drag | correct tip count and inner radius; sliders update live | kittest |
| UI-D06 | Line with 45° snap | draw with and without Shift/snap | angle constrained to 45° when active; free otherwise | kittest |
| UI-D07 | Bézier path | place three nodes, Enter | path with three nodes, handles consistent; Cancel discards | kittest |
| UI-D08 | Freehand stroke simplification | draw with Pencil, Simplify path | node count drops, shape stays within tolerance, one undo | kittest |
| UI-D09 | Text tool | click, type, set size/alignment | text object with expected bounds/content; editor reopens it | kittest |
| UI-D10 | Barcode create and invalid input | F8 strings, different symbologies | bars/labels match the encoded value; invalid string reports without object | kittest |
| UI-D11 | Measure tool | drag, Shift-drag | readout shows distance/angle; nothing added to the document | kittest |
| UI-D12 | Radius tool | arm Radius, click a corner | corner rounded by the current radius, one undo, status reports it | kittest |
| UI-D13 | Draw on a locked/active layer | lock the only layer, draw | creation refused with a status message, no object | kittest |
| UI-D14 | Pen/pan tools switch cleanly | switch tools repeatedly with pending paths | pending path cancelled or kept as described, no stuck drag | kittest |

### 5.8 Canvas, navigation and nodes (UI-N)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-N01 | Zoom in/out, fit, status | toolbar `+`/`-`/Fit and View > Zoom | zoom percentage and status update; fit frames the page | kittest |
| UI-N02 | Zoom to selection | select a shape, View > Zoom > Zoom to selection | view frames the selection; disabled with no selection | kittest |
| UI-N03 | Pan and mouse wheel | drag Pan/space, wheel zoom | canvas moves around the cursor; no object created | MANUAL (gestures) |
| UI-N04 | Marquee selection | drag on empty canvas | objects intersecting the marquee selected; Shift adds | kittest |
| UI-N05 | Nudge with arrows | select object, arrow keys with/without Shift | small/large step, one undo per run, no accidental move of locked object | MANUAL (key repeat) |
| UI-N06 | Grid and snapping toggles | View > Display/Snapping | grid/rulers/guides/selection visibility toggle; snap targets change cursor behaviour | kittest |
| UI-N07 | Guides dialog | View > Display > Guides..., add H/V guides | guide appears at the position, duplicate rejected, delete works | kittest |
| UI-N08 | Rulers and cursor readout | move over canvas | status X/Y matches the pointer; rulers mark millimetres | MANUAL (visual) |
| UI-N09 | Node edit entry and drag | select path, Edit nodes, drag node | node moves, path updates, one undo; Esc/Exit leaves editing | kittest |
| UI-N10 | Insert/delete/convert nodes | double-click segment, Delete, smooth/corner | node count and shape change as expected, labels in the panel update | kittest |
| UI-N11 | Handles align and simplify | select node, align handles; simplify tolerance | handles straight; path within tolerance; undo restores | kittest |
| UI-N12 | Context menu on canvas | right-click empty vs object | canvas or object menu with correct enabled states; actions run | kittest |

### 5.9 Transform, arrange and docking (UI-T)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-T01 | Bring/send order | four overlapping objects, To front/back/forward/backward | z-order changes one step at a time, status counts, undo | kittest |
| UI-T02 | Centre on page and move to position | use both | bounds centre equals page centre; exact X/Y applied with one undo | kittest |
| UI-T03 | Align six edges | multi-selection, six align buttons | each mode aligns to the selection/page correctly; disabled with no selection | kittest |
| UI-T04 | Distribute across and down | three unequal objects | equal gaps by centre, toolbar/menu agree | kittest |
| UI-T05 | Rotate 90° left/right | rotated/elliptical objects | bounds swap as expected, style kept, undo | kittest |
| UI-T06 | Flip horizontal/vertical | text and asymmetric path | mirrored around the selection centre, position kept | kittest |
| UI-T07 | Mirror across line | mirror across an angled line | geometry mirrored across the line, original/result positions correct | kittest |
| UI-T08 | Two-point rotate/scale | explicit pivot and two points + scale | rotation/scale match the points; pivot unchanged; undo | kittest |
| UI-T09 | Scale ±10 % and match width/height | mixed sizes | relative scale around the chosen centre; match uses first object | kittest |
| UI-T10 | Transform around | Selection/Page/Each centre | rotation/scaling centre changes visibly and numerically | kittest |
| UI-T11 | Reset transforms | scaled/rotated objects | geometry baked, bounds unchanged, transform identity, one undo | kittest |
| UI-T12 | Skew dialog | X/Y degrees, preview/apply | shear applied, bounds reflect it, Cancel unchanged | kittest |
| UI-T13 | Grid and circular arrays | dialogs with counts/spacing/rotation | correct copies, spacing, margins, rotate option; one undo | kittest |
| UI-T14 | Dock and nest | docking via MCP on a mixed selection; Nest dialog | dock packs on the chosen side with spacing, lock notes, one undo; nest fits the page without overlap | MCP + kittest |

### 5.10 Boolean and path operations (UI-B)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-B01 | Union/difference/intersection/xor | two overlapping closed shapes | one contour per result, area matches, two objects consumed, undo restores | kittest |
| UI-B02 | Boolean error with open path | open path selected | Clear error "not a closed shape", no change | kittest |
| UI-B03 | Weld several overlapping shapes | three shapes, Weld shapes | contours merged pairwise, status count, one undo | kittest |
| UI-B04 | Outline stroke | thick stroke, Outline stroke | stroke becomes closed outlines with the expected width, original removed | kittest |
| UI-B05 | Trim segments | crossing paths, Trim | segments cut at crossings; one undo; warning on unsupported geometry | kittest |
| UI-B06 | Delete overlapping segments | two paths sharing an edge | shared segment removed, paths split as described | kittest |
| UI-B07 | Offset positive/negative | closed path, Offset dialog | outward/inward contour with the requested distance; self-intersection handled | kittest |
| UI-B08 | Join and close paths | open paths with near ends | endpoints welded within tolerance; close adds the last segment; counts/status | kittest |
| UI-B09 | Reverse and start point | path with start marker | direction reverses; start point moves to the chosen node; reset restores | kittest |
| UI-B10 | Split at point and break apart | path, split; closed shape with hole, break apart | two paths at the point; hole becomes an independent object; text becomes outlines | kittest |
| UI-B11 | Cut shapes (cookie cutter) | closed cutter over targets | inside kept, outside discarded, targets consumed, one undo | kittest |
| UI-B12 | Shape Maker (MCP-only surface) | open chains + closed loops via `mcp: shape_maker` | chains close, loops union, holes linked, sources kept, one undo | MCP |
| UI-B13 | Fit text to path and text outlines | text plus a path | text laid along the path as outlines; original text replaced; undo | kittest |
| UI-B14 | Simplify and corner radius | freehand path and rectangle | node count drops within tolerance; radius applies/clears; one undo each | kittest |

### 5.11 Panels, layers and properties (UI-P)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-P01 | Layers panel layout and F9 | toggle panels | panel docks/resizes per preferences; F9 hides/shows; width persisted | MANUAL |
| UI-P02 | Add/delete/reorder layers | add three layers, drag/order buttons, delete empty | order changes, active layer follows, non-empty delete asks/handles objects | kittest |
| UI-P03 | Layer visibility and lock | toggle eye/lock | canvas and selection honour them; status explains skipped actions | kittest |
| UI-P04 | Layer colour palette | choose a colour | layer colour changes, object default colour follows, new objects use it | kittest |
| UI-P05 | Row menu: rename, duplicate, delete, merge | open the layer row menu | each action updates layers/objects once; duplicate copies objects; merge keeps order | kittest |
| UI-P06 | Object properties: size/position/rotation | edit W/H/X/Y/angle | geometry/transform updates, linked fields consistent, one undo | kittest |
| UI-P07 | Object style and operation | set stroke/fill/width/operation | canvas and cuts/layers update; preset operations listed; no invalid colour accepted | kittest |
| UI-P08 | Multi-selection properties | two objects of different sizes | common fields show mixed/blank, applying aligns or scales as described | kittest |
| UI-P09 | Document properties in the panel | no selection, edit name/size/unit | page/rulers/status update; undo restores | kittest |
| UI-P10 | Node properties while editing | node selected | coordinates, handles and smooth/corner state editable; changes undo | kittest |
| UI-P11 | Side panel tabs | switch Properties/Tool | tool options and live readout appear; pending drawing state kept | kittest |
| UI-P12 | Panel widths and reset | resize, Reset layout | widths/sides restore defaults; document unchanged | MANUAL (visual width) |

### 5.12 Operations, materials and machines (UI-O)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-O01 | Cuts/Layers list and inline edit | open dialog, edit speed/power/passes | rows and preview update, validation blocks negatives, one undo | kittest |
| UI-O02 | Add/duplicate/delete operation | add two, duplicate, delete | dependencies/assignments stay consistent; unused operation can be deleted | kittest |
| UI-O03 | Operation dependencies | set "after" another | preview order follows; cycle rejected with a message | kittest |
| UI-O04 | Assign operation to layer/objects | menu and row actions | counts match, status names the operation, undo | kittest |
| UI-O05 | Parameters editor | add/remove parameter, use `{p}` in a field | expression evaluated, invalid name/expression reports, job reflects value | kittest |
| UI-O06 | Operation expressions | set an expression on a numeric field | value preview updates, invalid expression reports, clearing restores literal | kittest |
| UI-O07 | Constraints dialog | add coincident/distance, solve, remove | geometry moves within tolerance, conflicting constraints report, one undo | kittest |
| UI-O08 | Recipes dialog | apply a recipe to a material | operation fields match; missing recipe error; undo | kittest |
| UI-O09 | Materials and operations library | filter, create, edit, delete | lists/counts update, profile save persists, delete in use warns | kittest |
| UI-O10 | Machine profiles | add/edit work area, select default | page origin/bounds checks use it; invalid area rejected; persistence | kittest |
| UI-O11 | Compare operations | select two | side-by-side differences highlighted; no document change | kittest |
| UI-O12 | Import/export profiles and machines | round-trip JSON/CSV | counts match, invalid file reports, existing entries handled per dialog | kittest |
| UI-O13 | Material test grid | sizes/counts/spacing | preview and created objects match; limits enforced | kittest |
| UI-O14 | Machine wizard first run | fresh preferences | wizard can be skipped/completed; skipping still allows drawing | MANUAL (first-run state) |

### 5.13 Settings, workspaces and shortcuts (UI-S)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-S01 | Preferences search filters sections | type "snap", "theme", gibberish | matching sections/controls shown; gibberish shows "No settings match" | kittest |
| UI-S02 | General settings | unit, font, confirmations | new documents/text follow; discard confirmation obeys; persisted | kittest |
| UI-S03 | Interface settings | theme, overlays, selection handles | changes apply live and persist; light/dark contrast readable | MANUAL (visual) |
| UI-S04 | Drawing/snapping settings | grid step, targets, tolerance | canvas snapping honours each toggle; invalid values blocked | kittest |
| UI-S05 | Image settings | import pixel limit/resampling | limit applies to the next import; status names it | kittest |
| UI-S06 | Files/backup settings | default folder, backup/autosave interval | next dialog starts there; backup/autosave files appear after the interval | MANUAL (timer) |
| UI-S07 | Machines settings | default machine/work area | new projects use it; invalid selection rejected | kittest |
| UI-S08 | Shortcut editor | rebind a command, conflict, reset | conflict message, new chord works, reset restores default; invalid key rejected | kittest |
| UI-S09 | Reset one section / all | change values, reset | only that section/all defaults restored; preferences file updated | kittest |
| UI-S10 | Dark mode from View | toggle View > Dark mode | theme changes across menus/dialogs, persists after restart | MANUAL (visual/restart) |
| UI-S11 | Workspaces save/apply | save current, hide panels, apply | visibility/overlays restored exactly; name validation; limit | kittest |
| UI-S12 | Interface presets | save/apply/delete a preset | theme, workspace and keys all applied; invalid preset rejected | kittest |
| UI-S13 | Toggle/reset side panels (MCP/core) | toggle panels, reset layout | both hidden then restored; reset visibility; no undo | kittest + MCP |
| UI-S14 | Preferences round-trip and corrupt file | restart; replace prefs with invalid JSON | values persist; corrupt file falls back to defaults with a note, no data loss | MANUAL (restart) |

### 5.14 Palette, help and about (UI-Q)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-Q01 | Command palette opens and focuses | `Ctrl+K`, View > Command palette | window opens with focused search, all categories listed | kittest |
| UI-Q02 | Search ranking and labels | type "export", "weld", "zzz" | matching commands with shortcuts; empty state for no match | kittest |
| UI-Q03 | Run a mutating command | palette "Weld shapes" with a valid selection | action runs, palette closes, undo label "Weld" | kittest |
| UI-Q04 | Disabled commands | palette "Paste" with empty clipboard | disabled/absent, no change | kittest |
| UI-Q05 | Shortcut window | Help > Keyboard shortcuts | all bindings listed, current custom chords shown, search/close | kittest |
| UI-Q06 | Contextual help buttons | open each window, click `?` | help topic opens with the matching title/text, closes cleanly | kittest |
| UI-Q07 | Tour | Help > Show the tour | steps advance/back/skip; no document change; last step closes | kittest |
| UI-Q08 | Assistant for agents | Help > Assistant for agents | current MCP commands/keys listed, copy works, text honest about scope | kittest |
| UI-Q09 | Licences | Help > Licenses | notice text scrolls, closes; matches third-party notices | kittest |
| UI-Q10 | About | Help > About BeamBurin | version, build target/commit, licence, link, no update check | kittest |
| UI-Q11 | About link opens the browser | click the project link | browser opens the repository; app stays responsive | MANUAL |
| UI-Q12 | Help on first run | fresh preferences | welcome/tour available, can be dismissed and does not block drawing | MANUAL |

### 5.15 Agents, MCP and reports (UI-A)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-A01 | MCP status and inspection | `mcp: get_status`, `inspect`, `list_objects` | document counts/layers/selection match the UI | MCP |
| UI-A02 | MCP mutation reflects in the UI | `mcp: add_rectangle`, check canvas/status | object appears with the same bounds and layer | MCP + kittest |
| UI-A03 | Atomic batch | `mcp: apply_batch` with one invalid step | nothing applied, error names the step, one/no undo as described | MCP |
| UI-A04 | Read-only and workspace policy | start `mcp --read-only`, then a mutation; start with a workspace | mutation refused; paths outside workspace refused | MCP |
| UI-A05 | Background jobs | `start_job` export G-code, poll, cancel | state/percent/results; cancel leaves no partial file | MCP |
| UI-A06 | Checkpoints | create/list/restore | restore is one undo and matches the snapshot; ring limited to 16 | MCP |
| UI-A07 | Reports window | run an export/import, open the report | summaries/warnings match the operation, copyable text, closes | kittest |
| UI-A08 | Working task window | start a large import/export | progress and cancel shown; UI stays responsive; completion status | MANUAL (timing) |

### 5.16 Errors, accessibility and persistence (UI-Z)

| ID | Objective | Steps | Expected | Driver |
|---|---|---|---|---|
| UI-Z01 | Every dialog closes with Esc | open each window in section 5, press Esc | closes without changing document or undo; modal guards ask first | kittest |
| UI-Z02 | Disabled states | no selection, one object, locked layer | toolbar/menu entries disabled or explain why; no panic | kittest |
| UI-Z03 | Invalid numeric fields | negative size, NaN, huge values | clamped/rejected with a message; no invalid object saved | kittest |
| UI-Z04 | Status bar live region | perform actions | status text updates and is exposed as a polite live label | kittest (accesskit) |
| UI-Z05 | Accessible names | inspect dialogs and panels | each field/button has a meaningful label, including icon-only buttons | kittest |
| UI-Z06 | Error messages are actionable | trigger unsupported import/export/file errors | message names the file/field/limit and suggests the fix | kittest |
| UI-Z07 | Unsaved changes guard | close/new/open with edits | Save/Discard/Cancel all correct; no silent data loss | kittest |
| UI-Z08 | Corrupt/oversized project | open malformed, truncated, future-version files | clear error, previous document intact, no crash | kittest |
| UI-Z09 | Preferences/project persistence | restart after changes | settings, recent files and project bytes survive; no stray temp files | MANUAL (restart) |
| UI-Z10 | Recovery after a crash | kill the process with edits | next start offers recovery; accept restores, decline leaves the last save | MANUAL |
| UI-Z11 | Keyboard-only navigation | Tab/Enter/Esc through dialogs | focus order sensible, no trap, all actions reachable | MANUAL |
| UI-Z12 | Screen reader pass | NVDA/Orca on a desktop session | window titles, status and form labels announced; no duplicate noise | MANUAL |

## 6. Execution order and priorities

1. **P0 — data safety:** UI-F02, F05–F09, Z07–Z10, S14.
2. **P1 — core drawing and editing:** UI-D, UI-E, UI-N, UI-T, UI-B, UI-P.
3. **P2 — output:** UI-V, UI-R, UI-G, UI-O, UI-A05.
4. **P3 — settings/help/agents:** UI-S, UI-Q, UI-A, UI-Z remaining.
5. **P4 — manual desktop checks:** every MANUAL case, one session per
   operating system in scope (Linux aarch64/x86_64, Windows x86_64).

Each area is a separate run so a failure can be attributed without rerunning the
whole plan.

## 7. Exit criteria and reporting

A case is PASS only with the expected observation recorded. A run is complete
when:

* every non-MANUAL case in the area has a verdict;
* every MANUAL case has a desktop note or an explicit deferral;
* every failure has a reproduction (steps, fixture, expected vs observed) and a
  severity (data loss, wrong output, blocked flow, cosmetic);
* the document is unchanged after dialog-only checks, and undo counts match the
  stated actions;
* files written by export cases open and contain the expected content.

The round report lists: build/commit, area, case ids, pass/fail/manual, defects
with severity, and the fixture versions.

## 8. Manual and desktop-only list

| Case | Why it is manual |
|---|---|
| UI-F12 | Closing the real window and the window-manager guard |
| UI-I10 | Drag and drop from the desktop |
| UI-E14 | System clipboard (image, SVG, text) |
| UI-N03 | Pan/zoom gestures and wheel |
| UI-N05 | Key-repeat nudge timing |
| UI-N08 | Visual ruler/cursor alignment |
| UI-P01 | Visual dock widths and F9 behaviour |
| UI-P12 | Panel resize appearance |
| UI-S03 | Light/dark contrast and icon legibility |
| UI-S06 | Autosave/backup timer |
| UI-S10 | Theme persistence across a restart |
| UI-S14 | Real preferences file restart/corruption |
| UI-Q11 | Browser handoff from About |
| UI-Q12 | First-run desktop welcome |
| UI-A08 | Background task responsiveness |
| UI-Z09 | Full application restart |
| UI-Z10 | Crash/kill and recovery prompt |
| UI-Z11 | Real keyboard focus traversal |
| UI-Z12 | Screen reader (NVDA/Orca) |
| UI-G12 | Export cancellation timing |
| UI-O14 | First-run machine wizard |

## 9. Risks and open points

* **MCP does not drive the egui menu tree.** BeamBurin's MCP server exposes the
  document and files, not menu clicks. Menu/dialog cases therefore use the
  in-repo `egui_kittest` harness; MCP is used for document/file assertions and
  for actions that exist only on the agent surface.
* **Features with no UI path yet.** Docking, `export_print`, Shape Maker and
  Trace Image are reachable through MCP but not through a menu in this build.
  They are covered as agent-surface cases (UI-A/UI-T/UI-B) and marked as a gap
  for a future UI round, not as missing behaviour.
* **Printing** produces a PDF and hands it to the system viewer; the printer,
  paper and driver are outside the application and manual.
* **Performance** cases use F5 but thresholds are indicative; a slower runner
  must record absolute times, not fail on a fixed budget.
* **Persistence** depends on the per-user preferences directory; the plan uses
  a disposable profile where possible and marks true restarts as manual.
* **No product comparison names** appear in this plan or in the repository.
