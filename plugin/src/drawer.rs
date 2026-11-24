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
    /// Magnet hole diameter and depth (mm).
    pub magnet: [f64; 2],
    pub screws: bool,
    pub origin: [f64; 2],
    /// Space between the pieces in the design: LAYOUT_GAP, or 0 in a picked drawer.
    pub gap: f64,
    pub name: String,
    /// Build only this piece (numbered across X, then Y, from 1), or all of them (0).
    pub piece: u32,
    /// Cells left out, across the whole drawer: column and row from 1, from the front left.
    pub leave_out: Vec<[u32; 2]>,
    /// The outline of a picked floor that isn't a rectangle, as segments seen from above (else
    /// empty): where cells are left out, the floor beside the cells kept is filled up to it.
    pub outline: Vec<[[f64; 2]; 2]>,
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

/// How far the filler beside a notch stays from the floor's edge: as the plate does from the walls.
pub const FLOOR_FILL_CLEARANCE: f64 = 0.25;
/// How much room short of the clearance a cell or half cell beside a notch may be. A cell is kept
/// and its edge cut back to the clearance; a half cell's pocket is that much short at the edge.
pub const SHORT: f64 = 0.3;

/// Commands a plan may have: a drawer is a long job, which SolidCraft allows 1,048,576 host calls,
/// and the plugin also makes a progress and a cancel check every 16 commands.
pub const MAX_CALLS: usize = 1_048_576 * 16 / 18 - 2;

/// The pieces, where they are built.
pub fn pieces(o: &DrawerOptions, pl: &DrawerPlan) -> Vec<Piece> {
    let [ax, ay] = &pl.axes;
    let [px, py] = o.pitch;
    let (kx, ky) = (ax.pieces.len(), ay.pieces.len());
    let (magnets, screws) = (o.magnets && !o.open, o.screws && !o.open);
    let h = plate_height(o.open, magnets, screws, o.magnet[1]);
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
                magnet: o.magnet,
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
                cuts: Vec::new(),
                round_cuts: false,
            });
        }
    }
    if !o.leave_out.is_empty() {
        add_left_out(o, pl, &mut v, &cx0, &cy0);
        add_trims(o, pl, &mut v, &cx0, &cy0);
    }
    if !o.outline.is_empty() {
        add_filler_cuts(o, &mut v);
    }
    v
}

/// On a floor that isn't a rectangle: the plate's own filler along its outer edges, a cell's
/// length at a time (and the corners), is cut away where it isn't on the floor, the clearance in
/// from its edge (a rounded corner of the drawer, say).
fn add_filler_cuts(o: &DrawerOptions, v: &mut [Piece]) {
    let c = FLOOR_FILL_CLEARANCE - 0.01;
    for p in v.iter_mut() {
        let Block { corner: [x0, y0], nx, ny, pitch: [px, py] } = p.cells;
        let (x1, y1) = (x0 + px * nx as f64, y0 + py * ny as f64);
        let [fl, fr, ff, fb] = p.filler;
        // [x0, y0, x1, y1], and which sides are the plate's edge: left, right, front, back
        let mut segs: Vec<([f64; 4], [bool; 4])> = Vec::new();
        for k in 0..ny {
            let y = y0 + py * k as f64;
            if fl > 1e-6 { segs.push(([x0 - fl, y, x0, y + py], [true, false, false, false])); }
            if fr > 1e-6 { segs.push(([x1, y, x1 + fr, y + py], [false, true, false, false])); }
        }
        for k in 0..nx {
            let x = x0 + px * k as f64;
            if ff > 1e-6 { segs.push(([x, y0 - ff, x + px, y0], [false, false, true, false])); }
            if fb > 1e-6 { segs.push(([x, y1, x + px, y1 + fb], [false, false, false, true])); }
        }
        if fl > 1e-6 && ff > 1e-6 { segs.push(([x0 - fl, y0 - ff, x0, y0], [true, false, true, false])); }
        if fr > 1e-6 && ff > 1e-6 { segs.push(([x1, y0 - ff, x1 + fr, y0], [false, true, true, false])); }
        if fl > 1e-6 && fb > 1e-6 { segs.push(([x0 - fl, y1, x0, y1 + fb], [true, false, false, true])); }
        if fr > 1e-6 && fb > 1e-6 { segs.push(([x1, y1, x1 + fr, y1 + fb], [false, true, false, true])); }
        for (r, edge) in segs {
            let out = |side: usize, k: f64| if edge[side] { k } else { -0.01 };
            let test = [r[0] - out(0, c), r[1] - out(2, c), r[2] + out(1, c), r[3] + out(3, c)];
            if !on_floor(test, &o.outline) {
                let grow = |side: usize| if edge[side] { 0.5 } else { 0.0 };
                p.cuts.push([r[0] - grow(0), r[1] - grow(2), r[2] + grow(1), r[3] + grow(3)]);
            }
        }
    }
}

/// On a floor that isn't a rectangle: a kept cell beside a notch that comes closer to the floor's
/// edge than the clearance (up to SHORT closer is kept, see `cells_off_floor`) is cut back to it.
fn add_trims(o: &DrawerOptions, pl: &DrawerPlan, v: &mut [Piece], cx0: &[u32], cy0: &[u32]) {
    if o.outline.is_empty() {
        return;
    }
    let [ax, ay] = &pl.axes;
    let [px, py] = o.pitch;
    let kx = ax.pieces.len();
    let clearance = FLOOR_FILL_CLEARANCE;
    for j in 0..ay.pieces.len() {
        for i in 0..kx {
            let idx = j * kx + i;
            for lc in 0..ax.pieces[i] {
                for lr in 0..ay.pieces[j] {
                    let (c, r) = (cx0[i] + lc + 1, cy0[j] + lr + 1);
                    if o.leave_out.contains(&[c, r]) {
                        continue;
                    }
                    let x = v[idx].cells.corner[0] + px * lc as f64;
                    let y = v[idx].cells.corner[1] + py * lr as f64;
                    let cell = [x, y, x + px, y + py];
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let (nc, nr) = (c as i64 + dx, r as i64 + dy);
                        if nc < 1 || nr < 1 || !o.leave_out.contains(&[nc as u32, nr as u32]) {
                            continue;
                        }
                        // the cell with that side moved out by `e`, the others a hair in
                        let moved = |e: f64| {
                            let h = 0.01;
                            let mut t = [cell[0] + h, cell[1] + h, cell[2] - h, cell[3] - h];
                            match (dx, dy) {
                                (-1, _) => t[0] = cell[0] - e,
                                (1, _) => t[2] = cell[2] + e,
                                (_, -1) => t[1] = cell[1] - e,
                                _ => t[3] = cell[3] + e,
                            }
                            on_floor(t, &o.outline)
                        };
                        if moved(clearance) {
                            continue;
                        }
                        let (mut lo, mut hi) = (-SHORT - 0.01, clearance);
                        for _ in 0..24 {
                            let mid = (lo + hi) / 2.0;
                            if moved(mid) { lo = mid } else { hi = mid }
                        }
                        let back = clearance - lo;
                        let cut = match (dx, dy) {
                            (-1, _) => [cell[0] - 1.0, cell[1], cell[0] + back, cell[3]],
                            (1, _) => [cell[2] - back, cell[1], cell[2] + 1.0, cell[3]],
                            (_, -1) => [cell[0], cell[1] - 1.0, cell[2], cell[1] + back],
                            _ => [cell[0], cell[3] - back, cell[2], cell[3] + 1.0],
                        };
                        v[idx].cuts.push(cut);
                    }
                }
            }
        }
    }
}

/// The cuts for the drawer's cells left out. A cell's own piece cuts it out, past it where there
/// is nothing to keep beside it, and past a seam far enough to take its own tab too. A piece
/// beside it cuts off its tabs that reached into it.
fn add_left_out(o: &DrawerOptions, pl: &DrawerPlan, v: &mut [Piece], cx0: &[u32], cy0: &[u32]) {
    let [ax, ay] = &pl.axes;
    let [px, py] = o.pitch;
    let kx = ax.pieces.len();
    let (nx, ny) = (ax.cells as i64, ay.cells as i64);
    let gone = |c: i64, r: i64| c < 1 || r < 1 || c > nx || r > ny || o.leave_out.contains(&[c as u32, r as u32]);
    // the piece a cell is in, along one axis
    let at = |starts: &[u32], sizes: &[u32], k: i64| (0..sizes.len()).find(|&i| (starts[i] as i64) < k && k <= (starts[i] + sizes[i]) as i64);
    let seam = TAB_DEPTH + 1.0;
    for &[c, r] in &o.leave_out {
        let (c, r) = (c as i64, r as i64);
        let (Some(i), Some(j)) = (at(cx0, &ax.pieces, c), at(cy0, &ay.pieces, r)) else { continue };
        let own = j * kx + i;
        let (xa, ya) = (v[own].cells.corner[0] + px * (c - 1 - cx0[i] as i64) as f64, v[own].cells.corner[1] + py * (r - 1 - cy0[j] as i64) as f64);
        // how far past the cell to cut, on each side: (neighbour, its piece along that axis, filler there)
        let reach = |nc: i64, nr: i64, same: bool, filler: f64| {
            if nc < 1 || nr < 1 || nc > nx || nr > ny { filler + 0.5 }
            else if !same { seam }
            else if gone(nc, nr) { 0.5 }
            else { 0.0 }
        };
        let f = v[own].filler;
        let same_x = |k: i64| at(cx0, &ax.pieces, k) == Some(i);
        let same_y = |k: i64| at(cy0, &ay.pieces, k) == Some(j);
        v[own].cuts.push([
            xa - reach(c - 1, r, same_x(c - 1), f[0]),
            ya - reach(c, r - 1, same_y(r - 1), f[2]),
            xa + px + reach(c + 1, r, same_x(c + 1), f[1]),
            ya + py + reach(c, r + 1, same_y(r + 1), f[3]),
        ]);
        // the pieces on the left and in front put their tabs into this one's cells
        if c > 1 && !same_x(c - 1) && !gone(c - 1, r) {
            let left = &mut v[own - 1];
            let x1 = left.cells.corner[0] + px * left.cells.nx as f64;
            left.cuts.push([x1 - 0.01, ya, x1 + seam, ya + py]);
        }
        if r > 1 && !same_y(r - 1) && !gone(c, r - 1) {
            let front = &mut v[own - kx];
            let y1 = front.cells.corner[1] + py * front.cells.ny as f64;
            front.cuts.push([xa, y1 - 0.01, xa + px, y1 + seam]);
        }
    }
}

/// An edge seen from above, by its start, middle and end points (as `model.edges` gives them),
/// as points along it: two for a straight edge, and for an arc or a circle (start and end the
/// same) points every few degrees round the circle through the three.
pub fn edge_points(start: [f64; 2], mid: [f64; 2], end: [f64; 2]) -> Vec<[f64; 2]> {
    let d = |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let closed = d(start, end) < 1e-6;
    // the circle through the three points (or start and mid across it, when closed)
    let (centre, r) = if closed {
        ([(start[0] + mid[0]) / 2.0, (start[1] + mid[1]) / 2.0], d(start, mid) / 2.0)
    } else {
        let (ax, ay, bx, by, cx, cy) = (start[0], start[1], mid[0], mid[1], end[0], end[1]);
        let den = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
        if den.abs() < 1e-9 * (1.0 + d(start, end).powi(2)) {
            return vec![start, end];
        }
        let sq = |x: f64, y: f64| x * x + y * y;
        let ux = (sq(ax, ay) * (by - cy) + sq(bx, by) * (cy - ay) + sq(cx, cy) * (ay - by)) / den;
        let uy = (sq(ax, ay) * (cx - bx) + sq(bx, by) * (ax - cx) + sq(cx, cy) * (bx - ax)) / den;
        let r = d([ux, uy], start);
        // a mid point on the line (within a hundredth of a mm): straight
        if r > 1e6 {
            return vec![start, end];
        }
        ([ux, uy], r)
    };
    let ang = |p: [f64; 2]| (p[1] - centre[1]).atan2(p[0] - centre[0]);
    let tau = std::f64::consts::TAU;
    let (a0, am) = (ang(start), ang(mid));
    let norm = |a: f64| a.rem_euclid(tau);
    // the sweep from start through mid to end, counter-clockwise if mid is that way
    let sweep = if closed {
        tau
    } else {
        let (to_mid, to_end) = (norm(am - a0), norm(ang(end) - a0));
        if to_mid <= to_end { to_end } else { to_end - tau }
    };
    let dir = if closed && norm(am - a0) > std::f64::consts::PI + 1e-9 { -1.0 } else { 1.0 };
    let n = ((sweep.abs() / (5f64).to_radians()).ceil() as usize).max(2);
    (0..=n).map(|k| {
        let a = a0 + dir * sweep * k as f64 / n as f64;
        if k == n && !closed { end } else { [centre[0] + r * a.cos(), centre[1] + r * a.sin()] }
    }).collect()
}

/// Whether point `p` is inside the outline (its edges as segments), by counting crossings.
fn inside(p: [f64; 2], outline: &[[[f64; 2]; 2]]) -> bool {
    let mut odd = false;
    for [a, b] in outline {
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            odd = !odd;
        }
    }
    odd
}

/// Whether segment `s` passes through the open rectangle `r` ([x0, y0, x1, y1]).
fn crosses(s: &[[f64; 2]; 2], r: [f64; 4]) -> bool {
    // clip the segment to the rectangle (Liang-Barsky); it crosses when something is left
    let [a, b] = *s;
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d[0], a[0] - r[0]), (d[0], r[2] - a[0]), (-d[1], a[1] - r[1]), (d[1], r[3] - a[1])] {
        if p.abs() < 1e-12 {
            if q <= 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 { t0 = t0.max(t) } else { t1 = t1.min(t) }
        }
    }
    t1 - t0 > 1e-9
}

/// The drawer's cells that aren't wholly on a floor with this outline (its edges as segments,
/// seen from above), `clearance` from its edges: the ones to leave out when the floor isn't a
/// rectangle.
pub fn cells_off_floor(o: &DrawerOptions, outline: &[[[f64; 2]; 2]], clearance: f64) -> Result<Vec<[u32; 2]>, String> {
    let pl = plan(o)?;
    let [ax, ay] = &pl.axes;
    let o = DrawerOptions { leave_out: Vec::new(), ..o.clone() };
    let all = pieces(&o, &pl);
    let kx = ax.pieces.len();
    let starts = |pieces: &[u32]| pieces.iter().scan(0u32, |acc, &k| { let s = *acc; *acc += k; Some(s) }).collect::<Vec<_>>();
    let (cx0, cy0) = (starts(&ax.pieces), starts(&ay.pieces));
    let mut out = Vec::new();
    for (n, p) in all.iter().enumerate() {
        let (i, j) = (n % kx, n / kx);
        for lc in 0..p.cells.nx {
            for lr in 0..p.cells.ny {
                let x = p.cells.corner[0] + o.pitch[0] * lc as f64;
                let y = p.cells.corner[1] + o.pitch[1] * lr as f64;
                // a cell that comes up to SHORT short of the clearance is kept: its edge is cut back
                // (`add_trims`)
                let c = clearance - SHORT;
                if !on_floor([x - c, y - c, x + o.pitch[0] + c, y + o.pitch[1] + c], outline) {
                    out.push([cx0[i] + lc + 1, cy0[j] + lr + 1]);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Whether rectangle `r` ([x0, y0, x1, y1]) is wholly on the floor with this outline.
fn on_floor(r: [f64; 4], outline: &[[[f64; 2]; 2]]) -> bool {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]].iter().all(|&q| inside(q, outline))
        && !outline.iter().any(|s| crosses(s, r))
}

/// Filler on the floor beside a kept cell where the cell next to it is left out: a box joined to
/// the kept cell's piece, with half cells in it where 21 mm or more fits (42 mm grid).
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// [x0, y0, x1, y1]
    pub rect: [f64; 4],
    /// The piece it is joined to, by its index in `pieces`.
    pub piece: usize,
    pub half_cells: Option<Block>,
}

/// The strips for a drawer whose cells are left out because its floor isn't a rectangle: in each
/// cell left out, beside each kept cell next to it, the floor up to `clearance` from its edge.
/// Where two meet in a corner they overlap; only the deepest gets half cells, so none overlap.
pub fn floor_fill(o: &DrawerOptions, pl: &DrawerPlan, clearance: f64) -> Vec<Strip> {
    if o.outline.is_empty() || o.leave_out.is_empty() {
        return Vec::new();
    }
    let [ax, ay] = &pl.axes;
    let [px, py] = o.pitch;
    let kx = ax.pieces.len();
    let (nx, ny) = (ax.cells as i64, ay.cells as i64);
    let all = pieces(&DrawerOptions { leave_out: Vec::new(), ..o.clone() }, pl);
    let starts = |pieces: &[u32]| pieces.iter().scan(0u32, |acc, &k| { let s = *acc; *acc += k; Some(s) }).collect::<Vec<_>>();
    let (cx0, cy0) = (starts(&ax.pieces), starts(&ay.pieces));
    let at = |starts: &[u32], sizes: &[u32], k: i64| (0..sizes.len()).find(|&i| (starts[i] as i64) < k && k <= (starts[i] + sizes[i]) as i64);
    let kept = |c: i64, r: i64| c >= 1 && r >= 1 && c <= nx && r <= ny && !o.leave_out.contains(&[c as u32, r as u32]);
    // a cell's rectangle, and the piece it is in
    let cell = |c: i64, r: i64| -> ([f64; 4], usize) {
        let (i, j) = (at(&cx0, &ax.pieces, c).unwrap_or(0), at(&cy0, &ay.pieces, r).unwrap_or(0));
        let p = &all[j * kx + i];
        let x = p.cells.corner[0] + px * (c - 1 - cx0[i] as i64) as f64;
        let y = p.cells.corner[1] + py * (r - 1 - cy0[j] as i64) as f64;
        ([x, y, x + px, y + py], j * kx + i)
    };
    let half = o.half_cells && (px - PITCH).abs() < 1e-9 && (py - PITCH).abs() < 1e-9;
    let mut out = Vec::new();
    for &[c, r] in &o.leave_out {
        let (c, r) = (c as i64, r as i64);
        let (rc, _) = cell(c, r);
        let mut found: Vec<(f64, Strip)> = Vec::new();
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            if !kept(c + dx, r + dy) {
                continue;
            }
            let (_, owner) = cell(c + dx, r + dy);
            // the strip from the kept cell's side into this one, `depth` deep
            // the sides along the strip: shrunk by the clearance where the floor ends right there
            let mut side = [0.0f64, 0.0f64];
            let strip = |depth: f64, side: [f64; 2]| match (dx, dy) {
                (-1, _) => [rc[0], rc[1] + side[0], rc[0] + depth, rc[3] - side[1]],
                (1, _) => [rc[2] - depth, rc[1] + side[0], rc[2], rc[3] - side[1]],
                (_, -1) => [rc[0] + side[0], rc[1], rc[2] - side[1], rc[1] + depth],
                _ => [rc[0] + side[0], rc[3] - depth, rc[2] - side[1], rc[3]],
            };
            // tested with `far` past its far side, `out` past the sides along it, and a hair in
            // from the kept cell's side
            let test = |depth: f64, side: [f64; 2], out: [f64; 2]| {
                let [x0, y0, x1, y1] = strip(depth, side);
                let e = 0.01;
                let t = match (dx, dy) {
                    (-1, _) => [x0 + e, y0 - out[0], x1 + clearance, y1 + out[1]],
                    (1, _) => [x0 - clearance, y0 - out[0], x1 - e, y1 + out[1]],
                    (_, -1) => [x0 - out[0], y0 + e, x1 + out[1], y1 + clearance],
                    _ => [x0 - out[0], y0 - clearance, x1 + out[1], y1 - e],
                };
                on_floor(t, &o.outline)
            };
            let deepest = |side: [f64; 2]| {
                let full = if dx != 0 { px } else { py };
                if test(full, side, [-0.01; 2]) {
                    return full;
                }
                let (mut lo, mut hi) = (0.0, full);
                for _ in 0..24 {
                    let mid = (lo + hi) / 2.0;
                    if test(mid, side, [-0.01; 2]) { lo = mid } else { hi = mid }
                }
                lo
            };
            let lo = deepest(side);
            for s in 0..2 {
                let mut out = [-0.01; 2];
                out[s] = clearance;
                if lo > 0.0 && !test(lo, side, out) {
                    side[s] = clearance;
                }
            }
            let lo = if side != [0.0; 2] { deepest(side) } else { lo };
            let depth = (lo * 100.0).floor() / 100.0;
            if depth < 1.0 {
                continue;
            }
            // a half cell may come up SHORT short at the floor's edge
            let k = if half { ((depth + SHORT) / HALF_PITCH).floor() as u32 } else { 0 };
            let half_cells = (k > 0).then(|| match (dx, dy) {
                (-1, _) => Block { corner: [rc[0], rc[1]], nx: k, ny: 1, pitch: [HALF_PITCH, py] },
                (1, _) => Block { corner: [rc[2] - HALF_PITCH * k as f64, rc[1]], nx: k, ny: 1, pitch: [HALF_PITCH, py] },
                (_, -1) => Block { corner: [rc[0], rc[1]], nx: 1, ny: k, pitch: [px, HALF_PITCH] },
                _ => Block { corner: [rc[0], rc[3] - HALF_PITCH * k as f64], nx: 1, ny: k, pitch: [px, HALF_PITCH] },
            });
            found.push((depth, Strip { rect: strip(depth, side), piece: owner, half_cells }));
        }
        // the deepest first, with its half cells; the others only where they reach past it
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut first: Option<[f64; 4]> = None;
        for (_, s) in found {
            let Some(f) = first else {
                first = Some(s.rect);
                out.push(s);
                continue;
            };
            let [x0, y0, x1, y1] = s.rect;
            let inner = [x0.max(f[0]), y0.max(f[1]), x1.min(f[2]), y1.min(f[3])];
            if inner[2] <= inner[0] || inner[3] <= inner[1] {
                out.push(Strip { half_cells: None, ..s });
                continue;
            }
            let rests = crate::plate::rest([x0, y0, x1 - x0, y1 - y0], [inner[0], inner[1], inner[2] - inner[0], inner[3] - inner[1]]);
            for [x, y, w, h] in rests.into_iter().filter(|r| r[2] > 0.5 && r[3] > 0.5) {
                out.push(Strip { rect: [x, y, x + w, y + h], piece: s.piece, half_cells: None });
            }
        }
    }
    out
}

/// Commands for the strips joined to piece `p` (index `index` in `pieces`).
fn strip_commands(out: &mut Vec<Value>, strips: &[Strip], index: usize, p: &Piece) {
    for (n, s) in strips.iter().filter(|s| s.piece == index).enumerate() {
        let [x0, y0, x1, y1] = s.rect;
        out.push(cmd("solid.box", json!({"corner": [r4(x0), r4(y0), 0.0], "length": r4(x1 - x0), "width": r4(y1 - y0),
            "height": r4(p.h), "operation": "join", "targets": [p.name]})));
        if let Some(b) = s.half_cells {
            crate::plate::half_pockets(out, &format!("{} Floor fill {}", p.name, n + 1), &p.name, p, b);
        }
    }
}

/// Pieces with every cell left out: they aren't made.
pub fn pieces_left_out(o: &DrawerOptions, pl: &DrawerPlan) -> Vec<bool> {
    let [ax, ay] = &pl.axes;
    let mut v = Vec::new();
    let (mut y0, kx) = (0, ax.pieces.len());
    for &h in &ay.pieces {
        let mut x0 = 0;
        for &w in &ax.pieces {
            v.push((x0 + 1..=x0 + w).all(|c| (y0 + 1..=y0 + h).all(|r| o.leave_out.contains(&[c, r]))));
            x0 += w;
        }
        y0 += h;
    }
    debug_assert_eq!(v.len(), kx * ay.pieces.len());
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
    let gone = pieces_left_out(o, pl);
    pieces(o, pl).iter().enumerate().filter(|(n, _)| !gone[*n]).map(|(n, p)| {
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

/// A copy of body `from`, made at `at`, as body `name`.
fn copy(from: &str, name: &str, from_at: [f64; 2], at: [f64; 2]) -> Value {
    cmd("solid.move", json!({"bodies": [from], "copy": true, "body_name": name,
        "translate": [r4(at[0] - from_at[0]), r4(at[1] - from_at[1]), 0.0]}))
}

/// Whether piece `b` is piece `a` moved: built where `a` is, it gets the same commands.
fn same_piece(a: &Piece, b: &Piece) -> bool {
    // quick no first: most pieces differ in these
    if (a.cells.nx, a.cells.ny, a.filler, a.round, a.tabs, a.sockets) != (b.cells.nx, b.cells.ny, b.filler, b.round, b.tabs, b.sockets) {
        return false;
    }
    let shift = [a.cells.corner[0] - b.cells.corner[0], a.cells.corner[1] - b.cells.corner[1]];
    let moved = Piece {
        name: a.name.clone(),
        cells: Block { corner: a.cells.corner, ..b.cells },
        grid_origin: [b.grid_origin[0] + shift[0], b.grid_origin[1] + shift[1]],
        make_cell: true,
        cuts: b.cuts.iter().map(|[xa, ya, xb, yb]| [xa + shift[0], ya + shift[1], xb + shift[0], yb + shift[1]]).collect(),
        ..b.clone()
    };
    let (mut x, mut y) = (Vec::new(), Vec::new());
    piece_commands(&mut x, &Piece { make_cell: true, ..a.clone() });
    piece_commands(&mut y, &moved);
    x == y
}

/// Commands for every piece, and the plan they follow.
pub fn drawer_commands(o: &DrawerOptions) -> Result<(DrawerPlan, Vec<Value>), String> {
    let pl = plan(o)?;
    let (nx, ny) = (pl.axes[0].cells, pl.axes[1].cells);
    if let Some([c, r]) = o.leave_out.iter().find(|[c, r]| *c > nx || *r > ny) {
        return Err(format!("Leave out cells: {c}-{r} is outside the drawer, which is {nx} x {ny} cells"));
    }
    if o.leave_out.len() as u32 >= nx * ny {
        return Err("Leave out cells: that leaves no cells".into());
    }
    let gone = pieces_left_out(o, &pl);
    let mut all = pieces(o, &pl);
    let mut out = Vec::new();
    if o.piece > 0 {
        let count = all.len();
        if o.piece as usize > count {
            return Err(format!("this drawer has {count} pieces: Piece must be 0 (all) or 1 to {count}"));
        }
        if gone[o.piece as usize - 1] {
            return Err(format!("piece {} has all its cells left out", o.piece));
        }
        let mut p = all.swap_remove(o.piece as usize - 1);
        p.make_cell = true;
        piece_commands(&mut out, &p);
        strip_commands(&mut out, &floor_fill(o, &pl, FLOOR_FILL_CLEARANCE), o.piece as usize - 1, &p);
        return Ok((pl, out));
    }
    // the floor beside the cells kept, where the floor isn't a rectangle, filled last
    let strips = floor_fill(o, &pl, FLOOR_FILL_CLEARANCE);
    let mut fill = Vec::new();
    for (index, p) in all.iter().enumerate() {
        strip_commands(&mut fill, &strips, index, p);
    }
    // pieces with every cell left out aren't made
    let all: Vec<Piece> = all.into_iter().zip(&gone).filter(|(_, g)| !**g).map(|(p, _)| p).collect();
    // A piece the same as one before it is a copy of that one, made once it is built: a copy
    // is one command, a piece twenty or more. Big drawers have many such pieces in the middle.
    let mut copy_of: Vec<Option<usize>> = Vec::new();
    for j in 0..all.len() {
        let from = (0..j).find(|&i| copy_of[i].is_none() && same_piece(&all[i], &all[j]));
        copy_of.push(from);
    }
    let built: Vec<&Piece> = all.iter().zip(&copy_of).filter(|(_, c)| c.is_none()).map(|(p, _)| p).collect();
    // one cell, copied to the start of every other piece built before any of them grows
    first_cell(&mut out, built[0]);
    let c0 = built[0].cells.corner;
    for p in &built[1..] {
        out.push(copy(&built[0].name, &p.name, c0, p.cells.corner));
    }
    let mut seeds = Seeds::counting(&o.name);
    for p in &built {
        piece_commands_seeded(&mut Vec::new(), p, &mut seeds);
    }
    let mut seeds = seeds.ready();
    for p in &built {
        piece_commands_seeded(&mut out, p, &mut seeds);
    }
    for (p, from) in all.iter().zip(&copy_of) {
        if let Some(&i) = from.as_ref() {
            out.push(copy(&all[i].name, &p.name, all[i].cells.corner, p.cells.corner));
        }
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
    out.extend(fill);
    Ok((pl, out))
}

#[cfg(test)]
mod tests {
    use crate::geometry::MAGNET;
    use super::*;
    use crate::plate::{dovetail, CLEARANCE};
    use serde_json::Value;

    fn opts(drawer: [f64; 2], bed: [f64; 2]) -> DrawerOptions {
        DrawerOptions { drawer, bed, pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false, open: false, magnets: false, magnet: MAGNET, screws: false,
                        origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "Drawer".into(), piece: 0, leave_out: vec![], outline: vec![] }
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
    use crate::geometry::{expand, MAGNET, PITCH};

    #[test]
    fn a_big_drawer_stays_under_the_call_limit() {
        let o = DrawerOptions { drawer: [600.0, 500.0], bed: [256.0, 256.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false,
                                open: false, magnets: true, magnet: MAGNET, screws: true, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0, leave_out: vec![], outline: vec![] };
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
                                open: false, magnets: false, magnet: MAGNET, screws: false, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0, leave_out: vec![], outline: vec![] };
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
    fn a_u_shaped_floor_leaves_its_notch_out() {
        // 300 x 200, with a notch 100 wide and 80 deep in the middle of the back
        let pts = [[0.0, 0.0], [300.0, 0.0], [300.0, 200.0], [200.0, 200.0], [200.0, 120.0], [100.0, 120.0], [100.0, 200.0], [0.0, 200.0]];
        let outline: Vec<[[f64; 2]; 2]> = (0..pts.len()).map(|k| [pts[k], pts[(k + 1) % pts.len()]]).collect();
        let o = DrawerOptions { drawer: [299.5, 199.5], bed: [256.0, 256.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false,
                                open: false, magnets: false, magnet: MAGNET, screws: false, origin: [0.25, 0.25], gap: 0.0, name: "D".into(), piece: 0, leave_out: vec![], outline: vec![] };
        let off = cells_off_floor(&o, &outline, 0.25).unwrap();
        // 7 x 4 cells from x 3.5 and y 15.5: columns 3 to 5 reach into the notch above y 120 (rows 3 and 4)
        assert_eq!(off, vec![[3, 3], [3, 4], [4, 3], [4, 4], [5, 3], [5, 4]]);
        let o = DrawerOptions { leave_out: off, outline, ..o };
        let (pl, c) = drawer_commands(&o).unwrap();
        assert!(c.iter().any(|v| v["params"]["name"].as_str().is_some_and(|n| n.ends_with("Left out"))));
        // beside the notch: 3.5 + 2 x 42 = 87.5 to 100 on the left (12.25 mm, solid), 213.5 + ... on the right
        let strips = floor_fill(&o, &pl, FLOOR_FILL_CLEARANCE);
        eprintln!("{strips:?}");
        assert!(!strips.is_empty());
    }

    #[test]
    fn arcs_and_circles_become_points_round_them() {
        assert_eq!(edge_points([0.0, 0.0], [5.0, 0.0], [10.0, 0.0]), vec![[0.0, 0.0], [10.0, 0.0]]);
        // a whole circle of radius 30 about (150, 125), as SolidCraft gives it
        let c = edge_points([180.0, 125.0], [120.0, 125.0], [180.0, 125.0]);
        assert!(c.len() > 60);
        assert!(c.iter().all(|p| ((p[0] - 150.0).powi(2) + (p[1] - 125.0).powi(2)).sqrt() - 30.0 < 1e-9));
        // a quarter circle from (10, 0) to (0, 10) about the origin
        let q = edge_points([10.0, 0.0], [10.0 * 0.5f64.sqrt(), 10.0 * 0.5f64.sqrt()], [0.0, 10.0]);
        assert_eq!((q[0], *q.last().unwrap()), ([10.0, 0.0], [0.0, 10.0]));
        assert!(q.len() >= 18 && q.len() < 25);
        // the same quarter the other way round
        let back = edge_points([0.0, 10.0], [10.0 * 0.5f64.sqrt(), 10.0 * 0.5f64.sqrt()], [10.0, 0.0]);
        assert!(back.iter().all(|p| p[0] >= -1e-9 && p[1] >= -1e-9));
    }

    #[test]
    fn many_pieces_fit_in_one_long_job() {
        let o = DrawerOptions { drawer: [1000.0, 1000.0], bed: [100.0, 100.0], pitch: [PITCH; 2], filler: [Filler::Even; 2], half_cells: false,
                                open: false, magnets: false, magnet: MAGNET, screws: false, origin: [0.0, 0.0], gap: LAYOUT_GAP, name: "D".into(), piece: 0, leave_out: vec![], outline: vec![] };
        let (pl, c) = drawer_commands(&o).unwrap();
        assert_eq!(pl.axes[0].pieces.len() * pl.axes[1].pieces.len(), 169);
        assert!(c.len() <= MAX_CALLS, "{} calls", c.len());
        // 16 kinds of piece (corners, edges and middle, 1 or 2 cells a side) are built; the rest
        // are copies of them
        let copies = c.iter().rev().take_while(|v| v["command"] == "solid.move" && v["params"]["copy"] == true).count();
        assert_eq!(copies, 169 - 16);
    }
}

