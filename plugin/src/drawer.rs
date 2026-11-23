//! A baseplate that fills a drawer: as many cells as fit, filler around them, cut along the
//! grid lines into pieces that fit the printer, joined by dovetail tabs.
//!
//! Each piece is built where it will lie in the design, laid out with a gap (or together, in a
//! drawer whose floor was picked): as a baseplate
//! (see plate.rs), with filler on the drawer's outside and a tab or socket at every cell along
//! each seam. The first cell is made once and copied to the start of every other piece.

use crate::geometry::{cmd, r4, Block, HALF_PITCH, PITCH};
use crate::plate::{first_cell, piece_commands, piece_commands_seeded, plate_height, Piece, Seeds, TAB_DEPTH};
use serde_json::{json, Value};

/// Space between the pieces as they are laid out in the design (mm), unless they are built
/// together in a picked drawer.
pub const LAYOUT_GAP: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filler {
    /// Split evenly between both sides.
    Even,
    /// All on the low side (left or front).
    Low,
    /// All on the high side (right or back).
    High,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawerOptions {
    /// Inside size of the drawer (mm).
    pub drawer: [f64; 2],
    /// Printer build plate (mm).
    pub bed: [f64; 2],
    pub pitch: [f64; 2],
    pub filler: [Filler; 2],
    /// Half cells in the leftover space where they fit.
    pub half_cells: bool,
    pub open: bool,
    pub magnets: bool,
    pub screws: bool,
    pub origin: [f64; 2],
    /// Space between the pieces in the design: LAYOUT_GAP, or 0 in a picked drawer.
    pub gap: f64,
    pub name: String,
    /// Build only this piece (numbered across X, then Y, from 1), or all of them (0).
    pub piece: u32,
}

/// One axis of the plan: cells, filler before and after them, and the pieces.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisPlan {
    pub cells: u32,
    pub filler_low: f64,
    pub filler_high: f64,
    /// Cells in each piece, low to high.
    pub pieces: Vec<u32>,
}

impl AxisPlan {
    /// Edges of the pieces along this axis, from 0 to the drawer size: `pieces.len() + 1` values.
    pub fn edges(&self, pitch: f64) -> Vec<f64> {
        let mut v = vec![0.0];
        let mut x = self.filler_low;
        for (i, &k) in self.pieces.iter().enumerate() {
            x += k as f64 * pitch;
            v.push(if i + 1 == self.pieces.len() { x + self.filler_high } else { x });
        }
        v
    }

    /// Size of each piece including the tabs on its high side.
    pub fn widths(&self, pitch: f64) -> Vec<f64> {
        let e = self.edges(pitch);
        let last = self.pieces.len() - 1;
        (0..=last).map(|i| e[i + 1] - e[i] + if i < last { TAB_DEPTH } else { 0.0 }).collect()
    }
}

/// Split one axis: the fewest pieces that fit `bed`, then the most even sizes. Errors say what
/// to change. With `half` and room for a half cell (21 mm), the half cells go on one side: the
/// one `filler` names, or for `Even` the high side, with what is left over split evenly.
pub fn plan_axis(size: f64, pitch: f64, bed: f64, filler: Filler, half: bool, axis: &str) -> Result<AxisPlan, String> {
    let n = ((size + 1e-9) / pitch).floor() as u32;
    if n == 0 {
        return Err(format!("the drawer's {axis} ({size} mm) is smaller than one cell ({pitch} mm)"));
    }
    let rest = size - n as f64 * pitch;
    let half = half && pitch == PITCH && rest + 1e-6 >= HALF_PITCH;
    let (lo, hi) = match filler {
        Filler::Even if half => ((rest - HALF_PITCH) / 2.0, (rest - HALF_PITCH) / 2.0 + HALF_PITCH),
        Filler::Even => (rest / 2.0, rest / 2.0),
        Filler::Low => (rest, 0.0),
        Filler::High => (0.0, rest),
    };
    // width of a piece holding cells i..j (j exclusive), with its filler and tab
    let width = |i: u32, j: u32| {
        let mut w = (j - i) as f64 * pitch;
        if i == 0 {
            w += lo;
        }
        if j == n {
            w += hi;
        } else {
            w += TAB_DEPTH;
        }
        w
    };
    for k in 1..=n {
        // best[j][p]: smallest possible largest piece covering cells 0..j with p pieces, and the cut
        let inf = f64::INFINITY;
        let mut best = vec![vec![(inf, 0u32); k as usize + 1]; n as usize + 1];
        best[0][0] = (0.0, 0);
        for p in 1..=k as usize {
            for j in 1..=n {
                for i in (p as u32 - 1)..j {
                    let prev = best[i as usize][p - 1].0;
                    if prev == inf {
                        continue;
                    }
                    let w = width(i, j);
                    if w > bed + 1e-9 {
                        continue;
                    }
                    let m = prev.max(w);
                    if m < best[j as usize][p].0 - 1e-9 {
                        best[j as usize][p] = (m, i);
                    }
                }
            }
        }
        if best[n as usize][k as usize].0 < inf {
            let mut pieces = Vec::new();
            let (mut j, mut p) = (n, k as usize);
            while p > 0 {
                let i = best[j as usize][p].1;
                pieces.push(j - i);
                j = i;
                p -= 1;
            }
            pieces.reverse();
            return Ok(AxisPlan { cells: n, filler_low: lo, filler_high: hi, pieces });
        }
    }
    Err(format!(
        "a piece doesn't fit the printer's {axis} ({bed} mm): one cell plus the filler is {:.1} mm; \
         put the filler on both sides or use a smaller grid",
        width(0, 1).max(width(n - 1, n))
    ))
}

#[derive(Debug)]
pub struct DrawerPlan {
    pub axes: [AxisPlan; 2],
}

pub fn plan(o: &DrawerOptions) -> Result<DrawerPlan, String> {
    Ok(DrawerPlan {
        axes: [
            plan_axis(o.drawer[0], o.pitch[0], o.bed[0], o.filler[0], o.half_cells, "width")?,
            plan_axis(o.drawer[1], o.pitch[1], o.bed[1], o.filler[1], o.half_cells, "depth")?,
        ],
    })
}

/// Commands a plan may have: SolidCraft allows 256 host calls a run, and the plugin also makes
/// a progress and a cancel check every 16 commands.
pub const MAX_CALLS: usize = 256 * 16 / 18 - 2;

/// The pieces, where they are built.
pub fn pieces(o: &DrawerOptions, pl: &DrawerPlan) -> Vec<Piece> {
    let [ax, ay] = &pl.axes;
    let [px, py] = o.pitch;
    let (kx, ky) = (ax.pieces.len(), ay.pieces.len());
    let (magnets, screws) = (o.magnets && !o.open, o.screws && !o.open);
    let h = plate_height(o.open, magnets, screws);
    let starts = |pieces: &[u32]| pieces.iter().scan(0u32, |acc, &k| { let s = *acc; *acc += k; Some(s) }).collect::<Vec<_>>();
    let (cx0, cy0) = (starts(&ax.pieces), starts(&ay.pieces));
    let name = |i: usize, j: usize| if kx * ky == 1 { o.name.clone() } else { format!("{} {}-{}", o.name, i + 1, j + 1) };
    // a piece's first cell, with its share of the layout gap
    let corner = |i: usize, j: usize| [
        o.origin[0] + ax.filler_low + px * cx0[i] as f64 + i as f64 * o.gap,
        o.origin[1] + ay.filler_low + py * cy0[j] as f64 + j as f64 * o.gap,
    ];
    let mut v = Vec::new();
    for j in 0..ky {
        for i in 0..kx {
            let c = corner(i, j);
            let (left, right, front, back) = (i == 0, i == kx - 1, j == 0, j == ky - 1);
            v.push(Piece {
                name: name(i, j),
                cells: Block { corner: c, nx: ax.pieces[i], ny: ay.pieces[j], pitch: o.pitch },
                h,
                open: o.open,
                magnets,
                screws,
                // holes line up with the piece's own grid, which is the drawer's moved by the gap
                grid_origin: [c[0] - px * cx0[i] as f64, c[1] - py * cy0[j] as f64],
                filler: [
                    if left { ax.filler_low } else { 0.0 },
                    if right { ax.filler_high } else { 0.0 },
                    if front { ay.filler_low } else { 0.0 },
                    if back { ay.filler_high } else { 0.0 },
                ],
                half_cells: o.half_cells,
                round: [left && front, right && front, right && back, left && back],
                tabs: [!right, !back],
                sockets: [!left, !front],
                make_cell: false,
            });
        }
    }
    v
}

/// A piece's label on the layout sheet: its number (along X, then Y, from 1), the centre of the
/// pocket in its middle, where the label goes so no grid line crosses it, and its size with the tabs.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub number: usize,
    pub at: [f64; 2],
    pub size: [f64; 2],
}

pub fn labels(o: &DrawerOptions, pl: &DrawerPlan) -> Vec<Label> {
    let (wx, wy) = (pl.axes[0].widths(o.pitch[0]), pl.axes[1].widths(o.pitch[1]));
    let kx = wx.len();
    pieces(o, pl).iter().enumerate().map(|(n, p)| {
        let (i, j) = (n % kx, n / kx);
        let mid = |k: u32, pitch: f64| ((k - 1) / 2) as f64 * pitch + pitch / 2.0;
        Label {
            number: n + 1,
            at: [p.cells.corner[0] + mid(p.cells.nx, p.cells.pitch[0]), p.cells.corner[1] + mid(p.cells.ny, p.cells.pitch[1])],
            size: [wx[i], wy[j]],
        }
    }).collect()
}

/// Half cells the run builds: all of the drawer's, or one piece's.
pub fn built_half_cells(o: &DrawerOptions, pl: &DrawerPlan) -> u32 {
    let all = pieces(o, pl);
    match o.piece {
        0 => all.iter().map(crate::plate::half_cell_count).sum(),
        k => all.get(k as usize - 1).map(crate::plate::half_cell_count).unwrap_or(0),
    }
}

/// Cells the run builds: all of the drawer's, or one piece's.
pub fn built_cells(o: &DrawerOptions, pl: &DrawerPlan) -> u32 {
    let [ax, ay] = &pl.axes;
    if o.piece == 0 {
        return ax.cells * ay.cells;
    }
    let i = (o.piece as usize - 1) % ax.pieces.len();
    let j = (o.piece as usize - 1) / ax.pieces.len();
    ax.pieces.get(i).copied().unwrap_or(0) * ay.pieces.get(j).copied().unwrap_or(0)
}

/// Commands for every piece, and the plan they follow.
pub fn drawer_commands(o: &DrawerOptions) -> Result<(DrawerPlan, Vec<Value>), String> {
    let pl = plan(o)?;
    let mut all = pieces(o, &pl);
    let mut out = Vec::new();
    if o.piece > 0 {
        let count = all.len();
        if o.piece as usize > count {
            return Err(format!("this drawer has {count} pieces: Piece must be 0 (all) or 1 to {count}"));
        }
        let mut p = all.swap_remove(o.piece as usize - 1);
        p.make_cell = true;
        piece_commands(&mut out, &p);
        return Ok((pl, out));
    }
    // one cell, copied to the start of every other piece before any of them grows
    first_cell(&mut out, &all[0]);
    let c0 = all[0].cells.corner;
    for p in &all[1..] {
        let c = p.cells.corner;
        out.push(cmd("solid.move", json!({"bodies": [all[0].name], "copy": true, "body_name": p.name,
            "translate": [r4(c[0] - c0[0]), r4(c[1] - c0[1]), 0.0]})));
    }
    let mut seeds = Seeds::counting(&o.name);
    for p in &all {
        piece_commands_seeded(&mut Vec::new(), p, &mut seeds);
    }
    let mut seeds = seeds.ready();
    for p in &all {
        piece_commands_seeded(&mut out, p, &mut seeds);
    }
    if out.len() > MAX_CALLS {
        let (kx, ky) = (pl.axes[0].pieces.len(), pl.axes[1].pieces.len());
        // each piece alone fits in a run when it can be built at all
        let one = all.iter().map(|p| {
            let mut v = Vec::new();
            piece_commands(&mut v, &Piece { make_cell: true, ..p.clone() });
            v.len()
        }).max().unwrap_or(0);
        if one <= MAX_CALLS {
            let count = kx * ky;
            return Err(format!(
                "this drawer is {count} pieces, more than SolidCraft lets one plugin run build: set Piece to 1, 2, ... {count} \
                 and run once for each"
            ));
        }
        return Err(format!(
            "{kx} x {ky} pieces is more than SolidCraft lets one plugin run build; use a bigger printer size, \
             a bigger grid or a smaller drawer"
        ));
    }
    Ok((pl, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plate::{dovetail, CLEARANCE};
    use serde_json::Value;

    fn opts(drawer: [f64; 2], bed: [f64; 2]) -> DrawerOptions {
        DrawerOptions { drawer, bed, pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false, open: false, magnets: false, screws: false,
                        origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "Drawer".into(), piece: 0 }
    }

    #[test]
    fn labels_sit_in_a_middle_pocket_and_count_along_x_first() {
        let o = opts([400.0, 300.0], [256.0, 256.0]);
        let pl = plan(&o).unwrap();
        let l = labels(&o, &pl);
        assert_eq!(l.len(), 4);
        assert_eq!(l.iter().map(|l| l.number).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
        // piece 2 is to the right of piece 1, piece 3 behind it
        assert!(l[1].at[0] > l[0].at[0] && (l[1].at[1] - l[0].at[1]).abs() < 1e-9);
        assert!(l[2].at[1] > l[0].at[1] && (l[2].at[0] - l[0].at[0]).abs() < 1e-9);
        // a pocket's centre: half a pitch past a grid line
        let ps = pieces(&o, &pl);
        for (p, l) in ps.iter().zip(&l) {
            let k = (l.at[0] - p.cells.corner[0]) / PITCH - 0.5;
            assert!((k - k.round()).abs() < 1e-9 && k >= 0.0 && k < p.cells.nx as f64);
        }
        let w: f64 = pl.axes[0].widths(PITCH).iter().sum();
        assert!((l[0].size[0] + l[1].size[0] - w).abs() < 1e-9);
    }

    #[test]
    fn cells_and_filler() {
        let a = plan_axis(400.0, 42.0, 256.0, Filler::Even, false, "width").unwrap();
        assert_eq!(a.cells, 9);
        assert!((a.filler_low - 11.0).abs() < 1e-9 && (a.filler_high - 11.0).abs() < 1e-9);
        // 9 cells in two pieces, as even as the filler allows: 4 + 5 (the 4 has filler and a tab)
        assert_eq!(a.pieces.len(), 2);
        assert_eq!(a.pieces.iter().sum::<u32>(), 9);
        assert!(a.widths(42.0).iter().all(|&w| w <= 256.0));
        let e = a.edges(42.0);
        assert_eq!((e[0], *e.last().unwrap()), (0.0, 400.0));
        let low = plan_axis(400.0, 42.0, 256.0, Filler::Low, false, "width").unwrap();
        assert_eq!((low.filler_low, low.filler_high), (22.0, 0.0));
    }

    #[test]
    fn fits_in_one_piece_when_it_can() {
        let a = plan_axis(250.0, 42.0, 256.0, Filler::Even, false, "width").unwrap();
        assert_eq!(a.pieces, vec![5]);
        // the most even split: 10 cells on a 180 mm bed -> 4 + 3 + 3 or similar, never 4 + 4 + 2
        let b = plan_axis(420.0, 42.0, 180.0, Filler::Even, false, "width").unwrap();
        assert_eq!(b.pieces.len(), 3);
        assert!(b.pieces.iter().max().unwrap() - b.pieces.iter().min().unwrap() <= 1);
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert!(plan_axis(30.0, 42.0, 256.0, Filler::Even, false, "width").unwrap_err().contains("smaller than one cell"));
        assert!(plan_axis(300.0, 42.0, 40.0, Filler::Even, false, "width").unwrap_err().contains("doesn't fit"));
    }

    #[test]
    fn pieces_have_tabs_and_sockets() {
        let (pl, c) = drawer_commands(&opts([400.0, 200.0], [256.0, 256.0])).unwrap();
        assert_eq!((pl.axes[0].pieces.len(), pl.axes[1].pieces.len()), (2, 1));
        // one cell made as piece 1-1, copied to the start of piece 2-1 before either grows
        assert!(c.iter().any(|v| v["params"]["body_name"] == "Drawer 1-1" && v["params"]["operation"] == "new"));
        let copy = c.iter().position(|v| v["command"] == "solid.move" && v["params"]["copy"] == true && v["params"]["body_name"] == "Drawer 2-1").unwrap();
        let first_pattern = c.iter().position(|v| v["command"] == "solid.pattern.rectangular").unwrap();
        assert!(copy < first_pattern);
        // a tab on the right of piece 1's row, a socket on the left of piece 2's: every row has one
        assert!(sketches_named(&c, "1-1 Tabs X") && sketches_named(&c, "2-1 Sockets X"));
        assert!(!sketches_named(&c, "1-1 Sockets X") && !sketches_named(&c, "2-1 Tabs X"));
        // filler: 400 - 9 x 42 = 22 mm, 11 on each side; 200 - 4 x 42 = 32, 16 front and back
        let boxes = c.iter().filter(|v| v["command"] == "solid.box").count();
        assert_eq!(boxes, 2 + 2 * 2);
        // a single piece has no seams
        let (_, one) = drawer_commands(&opts([200.0, 200.0], [256.0, 256.0])).unwrap();
        assert!(!one.iter().any(|v| v["command"] == "solid.move"));
        assert!(!sketches_named(&one, "Tabs X") && !sketches_named(&one, "Sockets Y"));
        // just piece 2: made from its own cell
        let mut o = opts([400.0, 200.0], [256.0, 256.0]);
        o.piece = 2;
        let (_, two) = drawer_commands(&o).unwrap();
        assert!(two.iter().any(|v| v["params"]["body_name"] == "Drawer 2-1" && v["params"]["operation"] == "new"));
        assert!(!two.iter().any(|v| v["command"] == "solid.move"));
        o.piece = 3;
        assert!(drawer_commands(&o).unwrap_err().contains("2 pieces"));
    }

    fn sketches_named(c: &[Value], end: &str) -> bool {
        c.iter().any(|v| v["command"] == "sketch.create" && v["params"]["name"].as_str().unwrap().ends_with(end))
    }

    #[test]
    fn socket_is_the_tab_grown_by_the_clearance() {
        let t = dovetail(0, 100.0, 50.0, 0.0);
        let s = dovetail(0, 100.0, 50.0, CLEARANCE);
        assert_eq!(t[1][0], 100.0 + TAB_DEPTH);
        assert!((s[1][0] - (100.0 + TAB_DEPTH + CLEARANCE)).abs() < 1e-9);
        // far end wider than the neck: it locks
        assert!(t[2][1] - t[1][1] > t[3][1] - t[0][1]);
        assert!(s[2][1] - s[1][1] > t[2][1] - t[1][1] + 2.0 * CLEARANCE - 1e-3);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::geometry::{expand, PITCH};

    #[test]
    fn a_big_drawer_stays_under_the_call_limit() {
        let o = DrawerOptions { drawer: [600.0, 500.0], bed: [256.0, 256.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false,
                                open: false, magnets: true, screws: true, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0 };
        let (pl, c) = drawer_commands(&o).unwrap();
        assert_eq!((pl.axes[0].cells, pl.axes[1].cells), (14, 11));
        assert!(c.len() <= MAX_CALLS, "{} calls", c.len());
        // the pieces' cells add up to the drawer's
        assert_eq!(pl.axes[0].pieces.iter().sum::<u32>(), 14);
        assert_eq!(pl.axes[1].pieces.iter().sum::<u32>(), 11);
        let pieces = pl.axes[0].pieces.len() * pl.axes[1].pieces.len();
        // a copy of the first cell for every other piece
        let moves = c.iter().filter(|v| v["command"] == "solid.move").count();
        assert_eq!(moves, pieces - 1);
        // one tab per row on every seam across X, one per column on every seam across Y
        let e = expand(&c);
        let shapes = |end: &str| -> usize {
            e.iter().enumerate().filter(|(_, v)| v["command"] == "sketch.create" && v["params"]["name"].as_str().unwrap().ends_with(end))
                .map(|(i, _)| e[i + 1..].iter().take_while(|v| v["command"] != "sketch.finish").count()).sum()
        };
        let (kx, ky) = (pl.axes[0].pieces.len(), pl.axes[1].pieces.len());
        assert_eq!(shapes("Tabs X"), (kx - 1) * ky);
        assert_eq!(shapes("Tabs Y"), (ky - 1) * 14);
        assert_eq!(shapes("Sockets Y"), (ky - 1) * 14);
    }

    #[test]
    fn half_cells_go_where_the_leftover_space_is() {
        // 400 mm: 9 cells and 22 mm over, a half cell on the right and 0.5 mm solid on each side
        let a = plan_axis(400.0, PITCH, 256.0, Filler::Even, true, "width").unwrap();
        assert!((a.filler_low - 0.5).abs() < 1e-9 && (a.filler_high - 21.5).abs() < 1e-9);
        let a = plan_axis(400.0, PITCH, 256.0, Filler::Low, true, "width").unwrap();
        assert_eq!((a.filler_low, a.filler_high), (22.0, 0.0));
        // too little room: as before
        let a = plan_axis(390.0, PITCH, 256.0, Filler::Even, true, "width").unwrap();
        assert_eq!((a.filler_low, a.filler_high), (6.0, 6.0));
        // the half cells are cut only into the pieces on that side
        let o = DrawerOptions { drawer: [400.0, 300.0], bed: [256.0, 256.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: true,
                                open: false, magnets: false, screws: false, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0 };
        let (_, c) = drawer_commands(&o).unwrap();
        let named = |end: &str| c.iter().filter(|v| v["command"] == "sketch.create" && v["params"]["name"].as_str().unwrap().contains(end)).count();
        // 400 x 300: 9 x 7 cells, 22 and 6 mm over: half cells along X only, on the right pieces
        let (pl, _) = drawer_commands(&o).unwrap();
        // one half cell made, and moved or copied into each right-hand piece's strip
        assert_eq!(named("Half cell 21x42 1"), 1);
        let moves = c.iter().filter(|v| v["command"] == "solid.move" && v["params"]["bodies"][0] == "D Half cell 21x42").count();
        assert_eq!(moves, pl.axes[1].pieces.len());
        assert_eq!(named("Half cells front") + named("Half cells back"), 0);
        assert!(c.len() <= MAX_CALLS, "{} calls", c.len());
    }

    #[test]
    fn too_many_pieces_is_refused_in_words() {
        let o = DrawerOptions { drawer: [1000.0, 1000.0], bed: [100.0, 100.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false,
                                open: false, magnets: false, screws: false, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0 };
        assert!(drawer_commands(&o).unwrap_err().contains("pieces"));
    }
}
