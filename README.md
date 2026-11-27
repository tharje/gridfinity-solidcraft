# Gridfinity for SolidCraft

A [SolidCraft](https://github.com/tangency-labs/solidcraft) plugin that makes
[Gridfinity](https://gridfinity.xyz) bins and baseplates, including baseplates that fill a drawer and
are split to fit your printer. The result is ordinary SolidCraft features:
one undo step, editable afterwards, and they keep working without the plugin.

![A 3×2×6 bin with dividers](docs/images/bin-3x2x6.png)

| Underside: feet with magnet and screw holes | 4×3 baseplate with magnet holes |
|---|---|
| ![Bin underside](docs/images/bin-3x2x6-bottom.png) | ![Baseplate](docs/images/baseplate-4x3.png) |

- **bin**: width × depth cells, height in units (7 mm), Gridfinity feet, 0.95 mm walls,
  stacking lip, optional magnet holes (6.5 × 2.4 mm, or the size you set), screw holes (3 mm) and dividers.
  The dividers are spaced so the compartments, and their label tabs, look the same width from
  above: the stacking lip reaches in over the side walls, so the outer compartments are a little
  wider at the floor (#14).
  Full grid (42 mm cells) or **half grid** (21 mm cells: four feet fit one baseplate pocket; magnet
  holes sit on the same spots as a full-grid baseplate's magnets), or a **custom cell size**.
- **Custom grid** (`grid`: "Custom (sizes below)"): your own cell size from 15 to 100 mm, set
  separately along X and Y (`custom_size_x`, `custom_size_y`), so bins and baseplates divide evenly
  into a drawer or organizer. Parts with the same custom size fit each other, but not standard
  Gridfinity. Magnet and screw holes sit 8 mm in from the cell edges, or one in the middle of cells
  under 29 mm.
- **Scoop and label tab**: a rounded floor along the front wall of each compartment
  (`inside_scoop`), so small parts slide out, and a shelf along the back wall at the top
  (`inside_label_tab`) for a label. Label tab is None, For sticker or Printed text (see Label
  text below). Scoop is None, Auto or Custom: Auto sizes it to the
  compartment (the smallest of the height inside, the depth / 2.5 and 18 mm), Custom uses
  `inside_scoop_radius`. The label tab is 12 mm wide (front to back) unless you set
  `inside_label_tab_width` (5–30 mm), for example to a little more than your label tape. Its
  underside slopes at 45° to the back wall; in a bin too low for that it slopes to the floor and
  the tab stands on a block, so a low bin gets the full width too. Scoop radius and Label tab width
  only show when they are used. Both follow the dividers; with them a bin can have up to 36
  compartments in one run. With a stacking lip, the lip would hang over the scoop and over the back
  of a sticker, so there the front wall (with a scoop) and the back wall (with a tab For sticker)
  are made 2.6 mm thick, flush with the lip. The side walls, and the back wall behind printed text,
  stay as they are: a 1×2×4 bin with a scoop and a tab for a sticker takes 33.6 cm³ instead of
  31.0 (38.3 with all walls at 2.6 mm).
- **Edit bin**: click a bin, and the dialog shows its settings to change (see [Edit bin](#edit-bin)).
- **Layout sheet** (SolidCraft 0.46.2): a drawer baseplate comes with a drawing of its pieces, each
  numbered and with its size, to print or export (see the drawer baseplate below).
- **Label text** (`inside_label_text`): text on the label tabs, one label per compartment
  separated by `;` (back row first, left to right; `M3x6; M3x8;; Washers` leaves the third one
  empty). It is a body of its own, so you can print it in another filament: in the slicer, give the
  label body its own filament. It either stands up from the tab or is sunk into it with its top
  flush with the tab, for a flat two-colour label (`inside_label_text_style`: "Raised" or "Flush
  (inlaid)"), by `inside_label_text_height` (0.4 mm by default, so one or two layers). The size
  (`inside_label_text_size`, capital height) is picked to fit the tab unless you set it, and
  `inside_label_text_font` takes any installed font (empty: SolidCraft's built-in one). The text
  fields only show, and are only used, with Label tab "Printed text".
  Each label becomes a **text parameter** (`Label_1`, `Label_2`, …, `Bin2_Label_1` for the next
  bin): change it in **Parameters** and the label follows, without running the plugin again. The
  size stays as it was fitted, so keep a new label about as long. The label's extrude shows a
  warning that its letters don't touch the label body: that is expected, they are kept as one body
  on purpose.
- **Height as a parameter**: every bin gets a `Height_units` parameter (`Bin2_Height_units` for
  the next bin, and so on). Change it in **Parameters** and the bin grows or shrinks with its walls,
  lip, dividers, fill and labels, without running the plugin again. Scoops and label tabs keep the
  size they were made with, and the body keeps its name.
- **Solid bins** (`inside`: "Solid (for custom cutouts)"): filled to the top of the walls with the
  stacking lip around, so you can sketch on the flat top and cut holders for your own tools
  ([example](examples/solid-holder.json)). Set a **fill height** (`inside_fill_height`, mm from the
  bottom of the bin) to stop the fill lower: above it the bin is hollow with walls and lip, so a bin
  stacked on top clears what sits in your cutouts.
- **Eco bins** (`inside`: "Eco (thin walls, hollow feet)"): 0.8 mm walls and floor and hollow
  feet (a 0.8 mm shell, open into the bin), for much less filament and print time: a 1×1×3 bin takes
  5.3 cm³ instead of 12.8. Dividers, scoops, label tabs and magnet holes work as in other bins;
  under dividers and scoops the feet stay solid, so they stand on something, and with magnet holes
  the hollow starts above them. No screw holes. Like the eco bins of the
  [FreeCAD Gridfinity Workbench](https://github.com/Stu142/FreeCAD-Gridfinity-Workbench).
- **Filler on bins** (`filler_left`, `filler_right`, `filler_front`, `filler_back`, in mm): the
  wall moves out on that side, so a bin at the edge of a baseplate reaches over the baseplate's
  filler instead of leaving it empty. The feet stay on the grid; the part outside it has a flat
  bottom that rests on the filler. The lip, the inside, dividers, scoops and label tabs grow with
  the bin, and a solid bin stays solid. **Position** is the corner of the bin with its filler, as
  on the baseplate: give a bin the baseplate's filler on the sides it shares, at the same corner,
  and it sits on the grid. The flat bottom is 4.75 mm above the print bed, so a wide filler needs
  support (support on build plate only).
- **Wall thickness** (`inside_wall`, 0.4–5 mm; 0: the standard 0.95 mm, or 0.8 mm on eco bins):
  thicker or thinner walls. They grow inward, so the outside, the feet and the stacking lip stay
  the same, and the inside, dividers, scoops and label tabs move in with them. A wall thicker than
  the lip (2.6 mm) gets a flat ledge under the lip instead of the slope; on eco bins the feet stay
  solid under a thick wall.
- **Real size in the name**: a bin without a body name of its own is named with its outside size,
  e.g. "Gridfinity bin 2x1x3 (83.5 x 41.5 x 25.1 mm)": the cells less the 0.5 mm gap, with filler,
  and the height with the stacking lip. If you change `Height_units` in Parameters afterwards, the
  name keeps the height it was made with.
- **baseplate**: width × depth pockets with the Gridfinity profile, full, half or custom grid, a solid
  bottom (optional magnet and screw holes) or an **open frame** (no bottom). Add **filler** on any side
  (`filler_left`, `filler_right`, `filler_front`, `filler_back`, in mm) to fill out a drawer
  without the drawer settings. Where 21 mm or more of filler fits, it gets **half cells** next to the
  cells: pockets half a cell wide and as long as the cells (21 × 42 mm beside them, 42 × 21 mm in
  front and behind, 21 × 21 mm in the corners), for half-width bins and half-grid 1 × 2 bins. Only
  the rest is solid; set **Leftover space** (`leftover`) to "Solid" for solid filler only. Half cells are made
  on the 42 and 21 mm grids, not on a custom one, and have no magnet or screw holes.
- **Leave out cells** (`leave_out`): a base plate that isn't a rectangle, to fit round a corner
  or an obstacle. List the cells to leave empty as column-row, counted from 1 at the front left,
  such as `1-3, 2-3, 4-1`, or click them in the model with the panel's **Layout** tab (below). Where
  a cell is left out, the plate ends just as at its outer edge, since a Gridfinity cell's edge is
  the plate's edge. The corners it leaves are rounded like the plate's own (4 mm): outer corners
  are cut round, and inner corners are filled in round (#2, thanks to kinghat). Cells that only
  meet at a corner become separate bodies. A drawer plate keeps its corners square, to fit its
  drawer.

  | The base plate form with Leave out cells | The plate it makes (back), and one with cells clicked away |
  |---|---|
  | ![Leave out cells in the form](docs/images/dialog-baseplate-leave-out.png) | ![Base plates with cells left out](docs/images/panel-layout.png) |

- **Magnet hole size** (`magnet_diameter`, `magnet_depth`): 6.5 × 2.4 mm by default, for 6 × 2 mm
  magnets. Magnets sold as 6 × 2 mm often come a little bigger or smaller, so bins, baseplates and
  drawer baseplates let you set the hole from 3.5 to 8 mm across and 1 to 4 mm deep (#17, thanks to
  JoeJambo). A deeper hole makes a baseplate's floor as much thicker, and an eco bin's hollow feet
  start higher. **Edit bin** changes them too (0 keeps the bin's).
- **Screw holes** (`holes_screw`) on a baseplate or drawer baseplate go where the magnet holes go,
  with or without magnets: an M3 hole (3.4 mm) through the floor, countersunk 90° from the underside
  (6.8 mm) for a flat-head screw (ISO 10642) that sits flush. The screw goes up into a bin's screw
  hole (3 mm, 6 mm deep) and holds it down. The floor is 3.05 mm with screw holes (use M3 × 8), and
  4.75 mm with magnet and screw holes (M3 × 10): the countersink sits under the magnet hole, with
  0.6 mm between them for the magnet to rest on. Put a magnet or a screw in each spot.

  | 2×2 baseplate from below, magnet and screw holes | Section through a row of holes |
  |---|---|
  | ![Screw holes from below](docs/images/baseplate-screw-holes.png) | ![Section through the holes](docs/images/baseplate-screw-section.png) |
- **drawer baseplate**: enter the inside size of a drawer and the size of your printer's bed. It
  fits as many cells as the drawer takes, fills the space left over with filler (split evenly or
  all on one side): a row of half cells where 21 mm or more is left over (on the side you choose,
  for Even on the right and at the back), the rest solid. Then it cuts the plate along the grid lines into pieces that fit the printer, as
  evenly as it can. The pieces lock together with dovetail tabs (0.15 mm clearance), which
  sit low in the wall between pockets so they don't show; the piece with the sockets goes on from
  above. Tabs and sockets lean in towards the top (0.67 mm over the tab's height), so a tab that
  comes down a little off finds its socket by itself instead of bending its edge.

  It runs as a long job (SolidCraft 0.48): it may take up to ten minutes, shows its progress at
  the bottom right of the view, and its red X cancels it and leaves the design as it was. So a big
  drawer is built in one go: 600 × 500 mm with magnet holes (9 pieces) takes about a minute.
  Pieces that are the same, as in the middle of a big drawer, are built once and copied: 1000 × 1000 mm
  with magnet holes (25 pieces, 9 different) takes about 80 s instead of 165. Like any
  long job its form has no preview. **Piece** builds just one piece, to make one again.

  ![A drawer baseplate being built, with its progress at the bottom right](docs/images/drawer-job-progress.png)

  With SolidCraft 0.44 or newer you can instead click the drawer's floor in your design
  (**Drawer floor**, `floor`). The plate then gets the floor's size less 0.5 mm, centred, sits on
  the floor, and its pieces lie together as they will in the drawer (otherwise they are laid out
  10 mm apart). Width, depth and position are not used then. The floor must be flat and face up.

  A floor that isn't a rectangle (an L or a U, a cut-off or rounded corner, a hole for a pipe)
  works too: every cell that isn't wholly on the
  floor, 0.25 mm in from its edges, is left out (**Leave out cells**, `leave_out`, which you can
  also fill in yourself), and a piece with no cells left isn't made. Beside the cells kept, the
  floor left over in a cell left out is filled up to 0.25 mm from its edge, as the plate's own
  filler is: with half cells where 21 mm or more fits (42 mm grid, Leftover space Half cells), the
  rest solid. A cell (or half cell) up to 0.3 mm short of that is kept, and the plate's edge there
  is cut back to 0.25 mm from the floor's: two half cells never stand where a whole cell fits. The
  plate's own filler along its outer edges is checked the same way, a cell's length at a time, so
  it stops short of a rounded corner. What is made is rectangles, so along a slanted or curved edge
  the plate steps, and some floor shows. The floor's outline is kept with the plate's settings (`floor_outline`).

  ![A drawer plate on a U-shaped floor](docs/images/drawer-u-floor.png)

  The plate stops at the walls, also when they are bodies of their own standing on a bigger floor
  (like *up to next*): from the middle of the floor it looks in each of the four directions for
  the nearest upright face that rises at least 10 mm above the floor, in any body. With no wall
  that way it goes to the floor's edge. A floor that isn't a rectangle is filled to the rectangle
  around it.

  **Layout sheet** (SolidCraft 0.46.2, `layout_sheet`, on by default): the drawer baseplate also makes
  a drawing, an A3 sheet with the pieces seen from above, each with its number and its size, so
  you know which piece goes where when you lay them in the drawer. Open it in the **Drawing**
  workspace to print it, or export it as SVG or PNG. For a drawer built a piece at a time, the
  sheet comes with the last piece.

  ![The layout sheet of a 400 × 300 mm drawer in 4 pieces](docs/images/layout-sheet.png)

  ![A drawer plate made by clicking the drawer's floor](docs/images/drawer-floor.png)

![Solid bin with custom cutouts](docs/images/solid-bin-cutouts.png)

| Half-grid 2×2 bin, underside | Open-frame baseplate |
|---|---|
| ![Half-grid bin](docs/images/half-bin-bottom.png) | ![Open baseplate](docs/images/baseplate-open.png) |

| Custom 40 × 25 mm cells: 2×1 bin | Custom 40 × 25 mm cells: 2×2 baseplate |
|---|---|
| ![Custom bin](docs/images/custom-bin.png) | ![Custom baseplate](docs/images/custom-baseplate.png) |

| 2×2×6 bin with dividers, 12 mm scoops and label tabs | Cut through: scoop at the front, label tab at the back |
|---|---|
| ![Scoop and label tab](docs/images/bin-scoop-label.png) | ![Section](docs/images/bin-scoop-section.png) |

| Eco 2×2×4 bin with a divider, scoops and label tabs | Cut through: hollow feet, solid under the scoop |
|---|---|
| ![Eco bin](docs/images/bin-eco.png) | ![Eco section](docs/images/bin-eco-section.png) |

| Bins with filler on a 3×3 baseplate with 12 mm filler all round (baseplate green) | Cut through the left edge: the foot in its pocket, the bin over the filler |
|---|---|
| ![Bins with filler](docs/images/bin-filler.png) | ![Filler section](docs/images/bin-filler-section.png) |

| Cut through three bins, from left to right with 0.4, 4 and 2 mm walls (the last with a divider, scoops and label tabs): the outside and the lip are the same |
|---|
| ![Wall thickness](docs/images/bin-wall-section.png) |

| Label text, raised (the text is its own body, shown red) | Label text, flush (inlaid) |
|---|---|
| ![Label text](docs/images/bin-label-text.png) | ![Flush label text](docs/images/bin-label-flush.png) |

| A 3-unit bin… | …set to 6 units in Parameters |
|---|---|
| ![3 units](docs/images/height-3.png) | ![6 units](docs/images/height-6.png) |

| The labels in Parameters | Label text in DejaVu Serif |
|---|---|
| ![Parameters](docs/images/label-parameters.png) | ![Font](docs/images/bin-label-font.png) |

![Label text fields](docs/images/dialog-label-text.png)

| Solid 3×2×6 bin filled to 20 mm, with cutouts | Cut through: solid fill below, open above |
|---|---|
| ![Fill height](docs/images/bin-fill-height.png) | ![Section](docs/images/bin-fill-height-section.png) |

| 4×3 baseplate with solid filler (Leftover space: Solid): 12 mm left and right, 25 mm at the back | The baseplate form with the filler fields |
|---|---|
| ![Baseplate with filler](docs/images/baseplate-filler.png) | ![Filler fields](docs/images/dialog-filler.png) |

| A 400 × 300 mm drawer on a 256 mm printer: four pieces, half cells along the right edge | Dovetails between two pieces, from above and below |
|---|---|
| ![Drawer baseplate](docs/images/drawer-400x300.png) | ![Tabs](docs/images/drawer-tabs.png) ![Sockets](docs/images/drawer-tabs-under.png) |

| 4×3 baseplate with magnet holes, 25 mm filler on the left and 30 mm in front: half cells |
|---|
| ![Half cells in the filler](docs/images/baseplate-half-cells.png) |

Dimensions follow the Gridfinity spec as implemented in gridfinity-rebuilt-openscad
([research/SPEC.md](research/SPEC.md)). Every shape was checked by exporting STL from SolidCraft and
measuring its cross-sections. Not yet printed: please report how it fits.

> Status: 1.6.2. Needs SolidCraft 0.49.1 or newer: from 0.49.1 a dialog can show its fields in the
> order the plugin gives them. 0.48 still runs it, but with the fields in alphabetical order. Use
> 0.50.0 if you can: 0.49 joins the cells slowly, and big bins or plates can hit the time limit.
> Tested with SolidCraft 0.49.1 and 0.50.0 through scripts/`solidcraft-cli` and in the desktop app's
> plugin dialog. SolidCraft's plugin runtime is itself experimental, so other versions may refuse the
> package. For SolidCraft 0.46.2 or 0.47, use [Gridfinity 1.5.0](../../releases/tag/v1.5.0); for 0.45, use
> [Gridfinity 1.4.1](../../releases/tag/v1.4.1); for 0.43 or 0.44,
> [Gridfinity 1.3.1](../../releases/tag/v1.3.1).

## Install
Download `gridfinity-1.6.2.zip` from the [latest release](../../releases/latest), then in SolidCraft:
**Plugins › Install plugin…** (or **Preferences › Plugins**) and pick the zip. From a script:

```json
{"commands": [{"command": "plugin.install", "params": {"path": "gridfinity-1.6.2.zip"}}]}
```

Add `"replace": true` to update an installed version. You can also unpack the zip into
`~/.local/share/solidcraft/plugins/` (Linux), `%LOCALAPPDATA%/SolidCraft/plugins` (Windows) or
`~/Library/Application Support/SolidCraft/plugins` (macOS).

## Use it in SolidCraft
Press **Bin**, **Base plate**, **Drawer plate** or **Edit bin** in the Plugins part of the Model ribbon
(SolidCraft 0.44), or open **Plugins › Gridfinity › Gridfinity bin…**, **Gridfinity baseplate…** or
**Gridfinity drawer baseplate…**. Fill in the form and press **Run**. Open a design first: from
the start page the form opens behind it (press Esc to see it). The result is ordinary features in
the History bar, in one folder named like the body (open the folder to edit a single step).

![The plugin's buttons in the ribbon](docs/images/ribbon.png)

![The bin's folder in the History bar](docs/images/timeline-folder.png)

| Plugins menu | The bin form |
|---|---|
| ![Plugins menu](docs/images/plugins-menu.png) | ![Bin dialog](docs/images/dialog-bin.png) |

![A bin on a custom 40 × 25 mm grid in SolidCraft after Run](docs/images/gui-custom-bin.png)

### Edit bin
**Edit bin** changes a bin the plugin made. Press it and click the bin: the fields fill in with the
bin's own size and settings, so change only what should change. A field you have changed is not
filled in again. In a script, a field left out or at **Keep** (0 or -1 for numbers) keeps the bin's own value. Run
removes the old bin (its folder in the History bar and its parameters) and makes it again in the same
place, with the same name, or the new automatic name with the new size. A height or label text
changed in **Parameters** since is kept too. Label text: leave it empty to keep the texts, or type
`-` to remove them. The label text's style (raised or flush, for a label in two colours), size,
height and font can be changed too; for the font, `-` goes back to SolidCraft's built-in one. If the new settings don't work (two labels for one compartment), nothing is
removed. It works on bins made with Gridfinity 1.4 or newer; an older bin has no settings stored, so
make it again with **Bin**. Features you added to the bin afterwards (a fillet, a move) used the old
bin, so they fail after an edit.

| Edit bin, filled in with the picked bin's settings | A bin made one cell wider with Edit bin |
|---|---|
| ![Edit bin dialog](docs/images/dialog-edit.png) | ![The edited bin](docs/images/gui-edit.png) |

In SolidCraft 0.45 the assistant chat and MCP clients see the plugin's entries as tools
(`gridfinity.bin`, `gridfinity.baseplate`, `gridfinity.drawer`, `gridfinity.edit`), so you can ask
for "a Gridfinity bin 3 × 2 with magnet holes"; the assistant asks before each run.

| The drawer baseplate form | Its four pieces in SolidCraft |
|---|---|
| ![Drawer dialog](docs/images/dialog-drawer.png) | ![Drawer pieces](docs/images/gui-drawer.png) |

### Limits
SolidCraft gives a plugin run 256 commands and 30 seconds. The plugin makes one cell and copies
it, so the number of commands stays small at any size; the time goes on joining the cells, about
0.15 s a cell on a 4-core desktop, more with magnet or screw holes. One run builds up to 100 cells
(a 10 × 10 baseplate in about 16 s), 42 with magnet holes, 25 with screw holes (20 with both). Bins take longer: up to 72 cells (8 × 9),
45 with magnet holes, 24 with screw holes (20 with both), and an eco bin up to 25 (5 × 5), 21 with magnet holes.
Dividers that cross each other, scoops and label tabs count in too, so a big bin with many
compartments may need fewer dividers. Past that the plugin says so before it starts. Half cells in the filler take longer than cells: a 4 × 4
baseplate with magnet holes and a ring of half cells around it takes about 14 s, and the plugin
counts them in when it decides; set Leftover space to Solid to leave them out. For a bigger baseplate use the drawer baseplate.

The drawer baseplate is a long job: SolidCraft gives it ten minutes and over a million commands.
The plugin plans for eight minutes, so a slower computer still finishes: 1600 cells, 672 with
magnet holes, 400 with screw holes, 320 with both. That is a 1000 × 1000 mm drawer of 42 mm cells
without holes. Past that, build it a piece at a time: set **Piece** to 1, 2, … (numbered along X,
then Y) and run once for each. Pieces built separately land where they would have with the others.

Label text is slow: the desktop app checks every letter, about half a second each, and each
compartment with a label tab costs about as much. Letters and digits (spaces are free) plus
compartments may add up to 50 in one run, so a bin with 6 compartments takes 44 letters of label
text, and one with 36 compartments 14 (about 20 seconds either way). Flush text is cut out of the
bin as well, so each letter counts twice: 22 letters with 6 compartments, 7 with 36.

### The Gridfinity panel
SolidCraft 0.48 lets a plugin dock a panel. Press **Panel** in the ribbon (or open **Plugins ›
Gridfinity › Gridfinity panel…**) and **Run**; it docks at the bottom of the window, with four tabs:

- **Parts**: every part the plugin made in the design, with its size, height and label. Click a row
  for all its settings. **Refresh** reads the design again.
- **Quick**: one click makes a common part beside what is already in the design: a 1 × 1 × 6 bin,
  a 2 × 1 × 6 bin with a scoop and a sticker tab, a 2 × 2 × 6 bin in four compartments, an eco bin
  3 × 1 × 3 and a 4 × 4 base plate.
- **Layout**: click **Remove a cell**, then a pocket of a base plate in the model, and the plate is
  made again without that cell (one undo step each). **Restore all cells**, then the plate, makes
  it whole again.
- **Settings**: magnet and screw holes, the stacking lip and hollow or eco bins for the Quick parts,
  kept in the design.

| Parts | Layout: two cells clicked away |
|---|---|
| ![The panel's Parts tab](docs/images/panel-parts.png) | ![A base plate with two cells left out](docs/images/panel-layout.png) |

| Quick | Settings |
|---|---|
| ![The panel's Quick tab](docs/images/panel-quick.png) | ![The panel's Settings tab](docs/images/panel-settings.png) |

A panel can't open the plugin's own forms or tell its buttons which row is picked yet, so a bin is
still changed with **Edit bin** in the ribbon
([SolidCraft #21538](https://github.com/tangency-labs/solidcraft/issues/21538)).

## Run from a script
SolidCraft runs plugin code only when you ask: inspect the package to get its approval hash, then
run an entry with that hash ([examples/](examples/)).

```json
{"commands": [{"command": "plugin.inspect", "params": {"path": "/path/to/plugins/gridfinity"}}]}
```
```json
{"commands": [{"command": "plugin.run", "params": {
  "path": "/path/to/plugins/gridfinity", "approval": "<hash from plugin.inspect>", "entry": "bin",
  "params": {"width": 2, "depth": 1, "height": 3, "holes_magnet": true, "dividers_x": 1}}}]}
```
`solidcraft-cli run bin.json --out bin.stl` builds it and exports an STL.

| Entry | Parameters (default) |
|---|---|
| `bin` | `grid` "42 mm (full)" / "21 mm (half)" / "Custom (sizes below)", `custom_size_x`, `custom_size_y` 15–100 mm (42, Custom only), `width` up to 504 mm and 24 cells (1), `depth` (1), `height` 2–20 units (3), `inside` "Hollow" / "Eco (thin walls, hollow feet)" / "Solid (for custom cutouts)", `dividers_x` 0–10 (0), `dividers_y` 0–10 (0), `holes_magnet` (false), `magnet_diameter` 3.5–8 mm (6.5), `magnet_depth` 1–4 mm (2.4), `holes_screw` (false), `inside_scoop` "None" / "Auto" / "Custom" (None), `inside_scoop_radius` 1–40 mm (10, Custom only), `inside_label_tab` "None" / "For sticker" / "Printed text" (None; `true` and `false` from before still work in saved bins), `inside_label_tab_width` 5–30 mm (12, with a tab only), `inside_label_text` (labels separated by `;`), `inside_label_text_size` 0–20 mm (0: fit), `inside_label_text_height` 0.1–3 mm (0.4), `inside_label_text_style` "Raised" / "Flush (inlaid)", `inside_label_text_font` (installed font, empty: built-in), `inside_fill_height` mm (0: to the top; solid bins, from 6 mm to 1 mm under the inside top), `inside_wall` 0.4–5 mm (0: standard), `filler_left`, `filler_right`, `filler_front`, `filler_back` 0–200 mm (0), `position_x`, `position_y` mm (0, the corner of the bin with its filler), `stacking_lip` (true), `title` (body name; empty: automatic, with the real size) |
| `baseplate` | `grid`, `custom_size_x`, `custom_size_y`, `width` (3), `depth` (3), `bottom` "Solid" / "Open (frame only)", `holes_magnet` (false, solid only), `magnet_diameter`, `magnet_depth` (as on `bin`), `holes_screw` (false, solid only), `filler_left`, `filler_right`, `filler_front`, `filler_back` 0–200 mm (0), `leftover` "Half cells" / "Solid", `leave_out` ("", cells as column-row, e.g. "1-3, 4-1"), `position_x`, `position_y` mm (0, the corner of the plate with its filler), `title` |
| `edit` | `bin` (one picked body: `[{"type": "body", "name": "<bin's name>"}]`), and what to change: `cells_x` (width), `cells_y` (depth), `height` (0: keep; `width` and `depth` work too), `dividers_x`, `dividers_y` (-1: keep), `inside_scoop` ("Keep" / "None" / "Auto" / "Custom"), `inside_scoop_radius` mm (0: keep; Custom only), `inside_label_tab_width` mm (0: keep; not with Label tab "None"), `inside` ("Keep" or as on `bin`), `holes_magnet`, `magnet_diameter`, `magnet_depth` mm (0: keep), `holes_screw`, `stacking_lip`, `inside_label_tab` ("Keep" / "None" / "For sticker" / "Printed text"), `inside_label_text` (empty: keep, `-`: none; new text on a tab For sticker makes it Printed text; the text fields show with Keep and Printed text), `inside_label_text_style` ("Keep" or as on `bin`), `inside_label_text_size` mm (-1: keep), `inside_label_text_height` mm (0: keep), `inside_label_text_font` (empty: keep, `-`: built-in) |
| `drawer` | `drawer_width`, `drawer_depth` 15–1000 mm (300), `printer_x`, `printer_y` 50–1000 mm (256), `grid`, `custom_size_x`, `custom_size_y`, `bottom`, `holes_magnet`, `magnet_diameter`, `magnet_depth`, `holes_screw`, `filler_x` "Even (both sides)" / "Left" / "Right", `filler_y` "Even (both sides)" / "Front" / "Back", `leftover` "Half cells" / "Solid", `piece` (0: all), `leave_out` ("", cells across the drawer as column-row), `position_x`, `position_y` mm (0), `title`, `layout_sheet` (true), `floor` (SolidCraft 0.44+: one picked face, e.g. `[{"type": "face", "body": "Drawer", "index": 10}]`; sets the size and position) |

SolidCraft checks a script's settings against the forms, so use the names and choices above. A setting left out takes its default. The older short forms (`"full"`, `"half"`, `"custom"`, `"open"`, and the names `magnets`, `screws`, `name` and `origin: [x, y]`) are no longer accepted from scripts; parts made with them can still be edited.

Choices that differ slightly from the spec: the stacking lip has a 0.3 mm flat on top instead of a
knife edge, the floor inside a bin is at 6 mm (5.55 mm in an eco bin), and baseplate pockets keep a 0.05 mm rim between
them (SolidCraft's kernel can't taper profiles that touch).

## Build
`./build.sh` runs the tests, builds the `wasm32-unknown-unknown` module, wraps it as a component with
`wasm-tools component new` and zips the package into `package/`. Needs Rust with the
`wasm32-unknown-unknown` target and [wasm-tools](https://github.com/bytecodealliance/wasm-tools).

`package/gridfinity/tests/` holds test cases for SolidCraft 0.46.2 and later. `solidcraft-cli plugin test package/gridfinity` runs each one on the real host and checks the body count and volume. The tests are left out of the zip.

## Layout
- `plugin/` the Rust plugin: `src/geometry.rs` bins and shared shapes, `src/plate.rs` baseplates
  (and why they are built row by row), `src/drawer.rs` splitting a drawer into pieces, `src/lib.rs`
  the entries and the runner, `src/panel.rs` the docked panel (`cargo run --example panel_ui`
  prints its declarations for `plugin.json`); `cargo run --example plan -- drawer '{...}'` prints a plan's commands
- `gen/gridfinity.py` the reference generator (writes SolidCraft command scripts; `gen/reference/`
  holds its output); `gen/volumes.json` the volumes SolidCraft measured for the plugin's shapes,
  which match the generator's; `gen/stlcheck.py` measures an STL, `gen/render.py` makes the
  preview images
- `package/gridfinity/` the plugin package (`plugin.json` + `plugin.wasm`)
- `research/SPEC.md` the dimensions and where they come from

## Licence
MIT, see [LICENSE](LICENSE). `plugin/wit/plugin.wit` is SolidCraft's plugin interface (Apache-2.0);
see [NOTICE](NOTICE).
