//! Baseplates, built so they stay quick in a plugin run.
//!
//! In a plugin run SolidCraft spends time after every command in proportion to how big the
//! bodies it changed are (a command on a 6 x 6 plate costs 8 to 10 s, on a single row well under
//! one), and the whole run must finish in 30 s. So a plate is built from small bodies: one cell,
//! copied into a row; the row gets what every row has (filler at its ends, the tabs and sockets
//! of the seams across X); the row is copied for every row; the first and last rows get what only
//! they have (filler, rounded corners, the tabs and sockets of the seams across Y); and only then
//! are the rows joined, once.

use crate::geometry::{
    block_holes, block_rrects, circle, cmd, extrude, pattern, r4, sketch, Block, Op, HALF_PITCH, MAGNET_D, MAGNET_DEPTH,
    PITCH, PLATE_PROFILE, PLATE_R, PLATE_RIM, PLATE_SCREW_D, PLATE_SCREW_HEAD_D,
};
use serde_json::{json, Value};

/// How far a tab reaches into the neighbour, from the seam (mm). The wall beside a seam is
/// 2.15 mm wide half-way up the pocket, so tab plus clearance stays inside it.
pub const TAB_DEPTH: f64 = 1.8;
/// Half-width of a tab at the seam and at its far end: the far end is wider, so it locks.
pub const TAB_NECK: f64 = 3.0;
pub const TAB_HEAD: f64 = 4.5;
/// Gap left around a tab in the socket it fits into (mm, each side and at its end). Small, so
/// the pieces press together: 0.15 mm printed loose on an Elegoo CC2 in PETG.
pub const CLEARANCE: f64 = 0.05;
/// Gap above a tab, so the piece with the sockets still sits flat.
pub const CLEARANCE_TOP: f64 = 0.15;

/// Height of a baseplate: an open frame is just the pocket profile high (the pockets end at its
/// underside); a solid one keeps a floor, thicker with magnet or screw holes. With both, the
/// countersink sits under the magnet hole with 0.6 mm between them for the magnet to rest on.
pub fn plate_height(open: bool, magnets: bool, screws: bool) -> f64 {
    let (a, m, c) = PLATE_PROFILE;
    let depth = a + m + c;
    if open {
        return depth - PLATE_RIM;
    }
    depth + match (magnets, screws) {
        (true, true) => MAGNET_DEPTH + SCREW_SINK + 0.6,
        (false, false) => 0.35,
        _ => MAGNET_DEPTH + 0.6,
    }
}

/// Depth of a screw hole's 90° countersink.
pub const SCREW_SINK: f64 = (PLATE_SCREW_HEAD_D - PLATE_SCREW_D) / 2.0;

/// Screw holes under `cells` of the body or bodies `targets`: an M3 clearance hole from the
/// underside up through the floor (which is `floor` high), then its countersink.
fn screw_holes(out: &mut Vec<Value>, name: &str, cells: Block, grid_origin: [f64; 2], floor: f64, targets: Value) {
    let hole = format!("{name} Screws");
    sketch(out, 0.0, &hole, block_holes(cells, grid_origin, PLATE_SCREW_D));
    let mut e = extrude(&hole, floor + 0.1, Op::Cut(""), None, "positive");
    e["params"]["targets"] = targets.clone();
    out.push(e);
    // the cone runs on a little into the hole, so it doesn't end on the hole's edge
    let sink = format!("{name} Screw countersinks");
    sketch(out, 0.0, &sink, block_holes(cells, grid_origin, PLATE_SCREW_HEAD_D));
    let mut e = extrude(&sink, SCREW_SINK + 0.1, Op::Cut(""), Some(-45.0), "positive");
    e["params"]["targets"] = targets;
    out.push(e);
}

/// Tabs and sockets reach only as high as the straight part of the wall between pockets, so a
/// tab never shows in the pocket's top chamfer. The socket is a little taller and closed above:
/// the piece with the sockets goes on from above.
pub fn tab_height(h: f64) -> f64 {
    let (_, _, c) = PLATE_PROFILE;
    h - c + PLATE_RIM
}

/// Points of a dovetail tab (or, with `grow`, its socket) on a seam at `s` along `axis`
/// (0: a seam across X, the tab reaching +X; 1: across Y, reaching +Y), centred at `c`.
pub fn dovetail(axis: usize, s: f64, c: f64, grow: f64) -> Vec<[f64; 2]> {
    let slope = (TAB_HEAD - TAB_NECK) / TAB_DEPTH;
    let widen = grow * (1.0 + slope * slope).sqrt();
    // a tab starts 0.5 mm back inside the piece it belongs to, so it joins solidly; a socket
    // starts at the seam, so cutting it leaves the other piece alone
    let (u0, u1) = (if grow > 0.0 { s } else { s - 0.5 }, s + TAB_DEPTH + grow);
    let half = |u: f64| TAB_NECK + slope * (u - s) + widen;
    let pts = [[u0, c - half(u0)], [u1, c - half(u1)], [u1, c + half(u1)], [u0, c + half(u0)]];
    pts.iter().map(|&[u, v]| if axis == 0 { [r4(u), r4(v)] } else { [r4(v), r4(u)] }).collect()
}

fn polygon(pts: &[[f64; 2]]) -> Value {
    cmd("sketch.line", json!({"points": pts, "closed": true}))
}

/// One baseplate piece. Coordinates are where the piece is built.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub name: String,
    /// Its cells: corner of the first, counts and size.
    pub cells: Block,
    pub h: f64,
    pub open: bool,
    /// Magnet holes and screw holes (in the same spots), and the corner of the grid they line
    /// up with.
    pub magnets: bool,
    pub screws: bool,
    pub grid_origin: [f64; 2],
    /// Filler beside the cells: left, right, front, back (mm).
    pub filler: [f64; 4],
    /// Half cells in the filler where they fit (see `half_columns`), the rest solid.
    pub half_cells: bool,
    /// Rounded corners: front left, front right, back right, back left.
    pub round: [bool; 4],
    /// Tabs on the right and back seams, sockets on the left and front ones.
    pub tabs: [bool; 2],
    pub sockets: [bool; 2],
    /// Make the first cell (else a body of this name already holds it, see `first_cell`).
    pub make_cell: bool,
}

/// The half cells strips are copied from, one of each size (21 x 42 beside the cells, 42 x 21
/// in front of and behind them, 21 x 21 in the corners): each made once where it is first used
/// and moved into its last use (a copy is one command, a made half cell four). A plan is made
/// twice: first `counting`, to learn how often each is used.
pub struct Seeds {
    base: String,
    counting: bool,
    list: Vec<SeedCell>,
}

struct SeedCell {
    size: [f64; 2],
    at: Option<[f64; 2]>,
    /// Uses still to come.
    left: u32,
}

impl Seeds {
    pub fn counting(base: &str) -> Seeds {
        Seeds { base: base.to_string(), counting: true, list: Vec::new() }
    }

    /// After the counting pass: ready to make the half cells.
    pub fn ready(mut self) -> Seeds {
        self.counting = false;
        self
    }

    /// A half cell of `size` at `corner` as body `name` (or the seed itself, at its last use);
    /// returns the body's name.
    fn place(&mut self, out: &mut Vec<Value>, p: &Piece, corner: [f64; 2], size: [f64; 2], name: &str) -> String {
        let i = match self.list.iter().position(|c| c.size == size) {
            Some(i) => i,
            None => {
                self.list.push(SeedCell { size, at: None, left: 0 });
                self.list.len() - 1
            }
        };
        if self.counting {
            self.list[i].left += 1;
            return name.to_string();
        }
        let seed = format!("{} Half cell {}x{}", self.base, size[0], size[1]);
        let cell = &mut self.list[i];
        let at = *cell.at.get_or_insert_with(|| {
            out.push(cmd("solid.box", json!({"corner": [r4(corner[0]), r4(corner[1]), 0.0], "length": r4(size[0]),
                "width": r4(size[1]), "height": r4(p.h), "operation": "new", "body_name": seed})));
            half_pockets(out, &seed, &seed, p, Block { corner, nx: 1, ny: 1, pitch: size });
            corner
        });
        cell.left = cell.left.saturating_sub(1);
        let shift = [r4(corner[0] - at[0]), r4(corner[1] - at[1]), 0.0];
        if cell.left == 0 {
            if shift != [0.0; 3] {
                out.push(cmd("solid.move", json!({"bodies": [seed], "translate": shift})));
            }
            return seed;
        }
        out.push(cmd("solid.move", json!({"bodies": [seed], "copy": true, "body_name": name, "translate": shift})));
        name.to_string()
    }
}

/// Commands for one piece; see the module comment for the order.
pub fn piece_commands(out: &mut Vec<Value>, p: &Piece) {
    let mut seeds = Seeds::counting(&p.name);
    piece_commands_seeded(&mut Vec::new(), p, &mut seeds);
    piece_commands_seeded(out, p, &mut seeds.ready());
}

/// `piece_commands`, with half cells copied from `seeds`.
pub fn piece_commands_seeded(out: &mut Vec<Value>, p: &Piece, seeds: &mut Seeds) {
    let b = p.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let Block { corner: [x0, y0], nx, ny, pitch: [px, py] } = p.cells;
    let [fl, fr, ff, fb] = p.filler;
    let h = p.h;
    let half = px == crate::geometry::HALF_PITCH || py == crate::geometry::HALF_PITCH;

    if p.make_cell {
        first_cell(out, p);
    }
    // the first row
    if nx > 1 {
        out.push(cmd("solid.pattern.rectangular", json!({"bodies": [b], "dir1": [1, 0, 0], "count1": nx, "spacing1": r4(px)})));
        out.push(cmd("solid.combine", json!({"target": b, "tools": ["@created"], "operation": "join"})));
    }
    // what every row has: filler at its ends, the seams across X
    let x1 = x0 + px * nx as f64;
    let k = |f: f64| half_columns(p, f);
    let (kl, kr, kf, kb) = (k(fl), k(fr), k(ff), k(fb));
    let hp = HALF_PITCH;
    // half cells in the side filler sit next to the cells, as long as they are
    let sides = [
        (fl, x0 - fl, Block { corner: [x0 - hp * kl as f64, y0], nx: kl, ny: 1, pitch: [hp, py] }, "left"),
        (fr, x1, Block { corner: [x1, y0], nx: kr, ny: 1, pitch: [hp, py] }, "right"),
    ];
    let mut strips = Vec::new();
    for (f, x, cells, side) in sides {
        if f > 1e-6 && cells.nx > 0 {
            strips.push(json!(half_strip(out, p, seeds, [x, y0, f, py], &[cells], &n(&format!("Half cells {side}")))));
        } else if f > 1e-6 {
            out.push(cmd("solid.box", json!({"corner": [r4(x), r4(y0), 0.0], "length": r4(f), "width": r4(py), "height": r4(h),
                "operation": "join", "targets": [b]})));
        }
    }
    if !strips.is_empty() {
        out.push(cmd("solid.combine", json!({"target": b, "tools": strips, "operation": "join"})));
    }
    let tab_h = tab_height(h);
    let row_c = y0 + py / 2.0;
    if p.sockets[0] {
        sketch(out, 0.0, &n("Sockets X"), vec![polygon(&dovetail(0, x0 - fl, row_c, CLEARANCE))]);
        out.push(extrude(&n("Sockets X"), tab_h + CLEARANCE_TOP, Op::Cut(b), None, ""));
    }
    if p.tabs[0] {
        sketch(out, 0.0, &n("Tabs X"), vec![polygon(&dovetail(0, x1 + fr, row_c, 0.0))]);
        out.push(extrude(&n("Tabs X"), tab_h, Op::Join(b), None, ""));
    }
    // every row; then the first (front) and last (back) rows get their own things
    let last = if ny > 1 {
        out.push(cmd("solid.pattern.rectangular", json!({"bodies": [b], "dir1": [0, 1, 0], "count1": ny, "spacing1": r4(py)})));
        "@last"
    } else {
        b
    };
    let (left, right, front, back) = (x0 - fl, x1 + fr, y0, y0 + py * ny as f64);
    let col_c = x0 + px / 2.0;
    // the half cells along the front and back are as wide as the cells, and square in the
    // corners under the side ones
    let along = |y: f64, rows: u32| [
        Block { corner: [x0 - hp * kl as f64, y], nx: kl, ny: rows, pitch: [hp; 2] },
        Block { corner: [x0, y], nx, ny: rows, pitch: [px, hp] },
        Block { corner: [x1, y], nx: kr, ny: rows, pitch: [hp; 2] },
    ];
    // Front and back filler with half cells are bodies of their own, joined with the rows at the
    // end: each change to a row costs SolidCraft a check of it, and those add up.
    let mut late = Vec::new();
    let mut front_body = b.to_string();
    if kf > 0 {
        front_body = half_strip(out, p, seeds, [left, front - ff, right - left, ff], &along(front - hp * kf as f64, kf), &n("Half cells front"));
        late.push(json!(front_body));
    } else if ff > 1e-6 {
        out.push(cmd("solid.box", json!({"corner": [r4(left), r4(front - ff), 0.0], "length": r4(right - left), "width": r4(ff),
            "height": r4(h), "operation": "join", "targets": [b]})));
    }
    let front = front - ff;
    let back = back + fb;
    corners(out, &n("Corners front"), &front_body, h, &[(p.round[0], [left, front], [1.0, 1.0]), (p.round[1], [right, front], [-1.0, 1.0])]);
    if p.sockets[1] {
        let mut v = vec![polygon(&dovetail(1, front, col_c, CLEARANCE))];
        v.extend(pattern(nx, px, 1, 0.0));
        sketch(out, 0.0, &n("Sockets Y"), v);
        out.push(extrude(&n("Sockets Y"), tab_h + CLEARANCE_TOP, Op::Cut(b), None, ""));
    }
    // back row (the same body when there is one row)
    let mut back_body = last.to_string();
    if kb > 0 {
        back_body = half_strip(out, p, seeds, [left, back - fb, right - left, fb], &along(back - fb, kb), &n("Half cells back"));
        late.push(json!(back_body));
    } else if fb > 1e-6 {
        out.push(cmd("solid.box", json!({"corner": [r4(left), r4(back - fb), 0.0], "length": r4(right - left), "width": r4(fb),
            "height": r4(h), "operation": "join", "targets": [last]})));
    }
    corners(out, &n("Corners back"), &back_body, h, &[(p.round[2], [right, back], [-1.0, -1.0]), (p.round[3], [left, back], [1.0, -1.0])]);
    if p.tabs[1] {
        let mut v = vec![polygon(&dovetail(1, back, col_c, 0.0))];
        v.extend(pattern(nx, px, 1, 0.0));
        sketch(out, 0.0, &n("Tabs Y"), v);
        out.push(extrude(&n("Tabs Y"), tab_h, Op::Join(last), None, ""));
    }
    // half-grid holes differ from row to row, so they go into each row now
    let rows: Vec<&str> = if ny > 1 { vec![b, "@created"] } else { vec![b] };
    let floor = h - (PLATE_PROFILE.0 + PLATE_PROFILE.1 + PLATE_PROFILE.2 - PLATE_RIM);
    if p.magnets && half {
        sketch(out, floor, &n("Magnets"), block_holes(p.cells, p.grid_origin, MAGNET_D));
        let mut e = extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "negative");
        e["params"]["targets"] = json!(rows);
        out.push(e);
    }
    if p.screws && half {
        // `@created` still names the copied rows: a cut makes no bodies
        screw_holes(out, b, p.cells, p.grid_origin, floor, json!(rows));
    }
    // and only now one plate
    if ny > 1 {
        late.insert(0, json!("@created"));
    }
    if !late.is_empty() {
        out.push(cmd("solid.combine", json!({"target": b, "tools": late, "operation": "join"})));
    }
}

/// The first cell of `p`, as body `p.name`: a square block with its pocket and, when they are
/// the same in every cell, its magnet and screw holes.
pub fn first_cell(out: &mut Vec<Value>, p: &Piece) {
    let b = p.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let corner = p.cells.corner;
    let [px, py] = p.cells.pitch;
    let (a, m, c) = PLATE_PROFILE;
    let rim = PLATE_RIM;
    let h = p.h;
    let one = Block { corner, nx: 1, ny: 1, pitch: p.cells.pitch };
    sketch(out, 0.0, &n("Cell"), vec![cmd("sketch.rectangle.two_point",
        json!({"p0": [r4(corner[0]), r4(corner[1])], "p1": [r4(corner[0] + px), r4(corner[1] + py)]}))]);
    out.push(extrude(&n("Cell"), h, Op::New(b), None, ""));
    // The spec's pockets meet at knife edges; the kernel can't taper profiles that
    // touch, so each pocket keeps a 0.05 mm rim and its 45° starts 0.05 lower.
    let (r_top, r_mid) = (PLATE_R - rim, PLATE_R - c);
    sketch(out, h, &n("Pocket1"), block_rrects(one, px - 2.0 * rim, py - 2.0 * rim, r_top));
    out.push(extrude(&n("Pocket1"), c - rim, Op::Cut(b), Some(-45.0), "negative"));
    sketch(out, h - c + rim, &n("Pocket2"), block_rrects(one, px - 2.0 * c, py - 2.0 * c, r_mid));
    out.push(extrude(&n("Pocket2"), m, Op::Cut(b), None, "negative"));
    sketch(out, h - c + rim - m, &n("Pocket3"), block_rrects(one, px - 2.0 * c, py - 2.0 * c, r_mid));
    // An open frame's last cut runs a little past the underside so it goes right through.
    let last = if p.open { a + 0.2 } else { a };
    out.push(extrude(&n("Pocket3"), last, Op::Cut(b), Some(-45.0), "negative"));
    let half = px == crate::geometry::HALF_PITCH || py == crate::geometry::HALF_PITCH;
    let floor = h - (a + m + c - rim);
    if p.magnets && !half {
        sketch(out, floor, &n("Magnets"), block_holes(one, p.grid_origin, MAGNET_D));
        out.push(extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "negative"));
    }
    if p.screws && !half {
        screw_holes(out, b, one, p.grid_origin, floor, json!([b]));
    }
}

/// How many half cells `p` gets in its filler.
pub fn half_cell_count(p: &Piece) -> u32 {
    let [fl, fr, ff, fb] = p.filler.map(|f| half_columns(p, f));
    (fl + fr) * p.cells.ny + (ff + fb) * (p.cells.nx + fl + fr)
}

/// How many rows of half cells (21 mm) fit in filler `f` beside `p`'s cells: none unless the
/// piece asks for them and its grid is Gridfinity's (a half cell must line up with half bins).
pub fn half_columns(p: &Piece, f: f64) -> u32 {
    let gridfinity = p.cells.pitch.iter().all(|&s| s == PITCH || s == HALF_PITCH);
    if !p.half_cells || !gridfinity {
        return 0;
    }
    ((f + 1e-6) / HALF_PITCH).floor() as u32
}

/// Filler `[x, y, w, d]` with half cells in `blocks` (which together make a rectangle), as a
/// body of its own: half cells copied from `seeds`, and solid boxes for the rest, joined (each
/// pocket's cuts cost more than copies, and cuts into a bigger body more again). Returns the
/// body's name.
fn half_strip(out: &mut Vec<Value>, p: &Piece, seeds: &mut Seeds, rect: [f64; 4], blocks: &[Block], name: &str) -> String {
    let h = r4(p.h);
    let boxed = |[x, y, w, d]: [f64; 4]| json!({"corner": [r4(x), r4(y), 0.0], "length": r4(w), "width": r4(d), "height": h});
    let blocks: Vec<&Block> = blocks.iter().filter(|b| b.nx * b.ny > 0).collect();
    let mut bodies = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let body = seeds.place(out, p, b.corner, b.pitch, &format!("{name} {}", i + 1));
        if b.nx * b.ny > 1 {
            out.push(cmd("solid.pattern.rectangular", json!({"bodies": [body], "dir1": [1, 0, 0], "count1": b.nx,
                "spacing1": r4(b.pitch[0]), "dir2": [0, 1, 0], "count2": b.ny, "spacing2": r4(b.pitch[1]), "@aside": true})));
            out.push(cmd("solid.combine", json!({"target": body, "tools": ["@aside"], "operation": "join"})));
        }
        bodies.push(json!(body));
    }
    let x0 = blocks.iter().map(|b| b.corner[0]).fold(f64::INFINITY, f64::min);
    let y0 = blocks.iter().map(|b| b.corner[1]).fold(f64::INFINITY, f64::min);
    let x1 = blocks.iter().map(|b| b.corner[0] + b.pitch[0] * b.nx as f64).fold(f64::NEG_INFINITY, f64::max);
    let y1 = blocks.iter().map(|b| b.corner[1] + b.pitch[1] * b.ny as f64).fold(f64::NEG_INFINITY, f64::max);
    let first = bodies[0].as_str().unwrap_or_default().to_string();
    let mut tools: Vec<Value> = bodies[1..].to_vec();
    for (i, r) in rest(rect, [x0, y0, x1 - x0, y1 - y0]).into_iter().enumerate() {
        let rest_name = format!("{name} rest{}", i + 1);
        let mut b = boxed(r);
        b["operation"] = json!("new");
        b["body_name"] = json!(rest_name);
        out.push(cmd("solid.box", b));
        tools.push(json!(rest_name));
    }
    if !tools.is_empty() {
        out.push(cmd("solid.combine", json!({"target": first, "tools": tools, "operation": "join"})));
    }
    first
}

/// What is left of rectangle `outer` (`[x, y, w, d]`) around `inner`, as up to four rectangles:
/// the bands in front of and behind it, full width, then the pieces left and right of it.
pub fn rest(outer: [f64; 4], inner: [f64; 4]) -> Vec<[f64; 4]> {
    let [ox, oy, ow, od] = outer;
    let [ix, iy, iw, id] = inner;
    let (ox1, oy1, ix1, iy1) = (ox + ow, oy + od, ix + iw, iy + id);
    let mut v = Vec::new();
    let mut push = |r: [f64; 4]| if r[2] > 1e-6 && r[3] > 1e-6 { v.push(r) };
    push([ox, oy, ow, iy - oy]);
    push([ox, iy1, ow, oy1 - iy1]);
    push([ox, iy, ix - ox, id]);
    push([ix1, iy, ox1 - ix1, id]);
    v
}

/// Cut half-cell pockets, the same profile as a cell's, into `body` at `cells`. No magnet or
/// screw holes: they sit on the 42 mm grid, outside these cells.
fn half_pockets(out: &mut Vec<Value>, name: &str, body: &str, p: &Piece, cells: Block) {
    let (a, m, c) = PLATE_PROFILE;
    let rim = PLATE_RIM;
    let (h, [sx, sy]) = (p.h, cells.pitch);
    let shapes = |inset: f64, r: f64| block_rrects(cells, sx - 2.0 * inset, sy - 2.0 * inset, r);
    let (r_top, r_mid) = (PLATE_R - rim, PLATE_R - c);
    sketch(out, h, &format!("{name} 1"), shapes(rim, r_top));
    out.push(extrude(&format!("{name} 1"), c - rim, Op::Cut(body), Some(-45.0), "negative"));
    sketch(out, h - c + rim, &format!("{name} 2"), shapes(c, r_mid));
    out.push(extrude(&format!("{name} 2"), m, Op::Cut(body), None, "negative"));
    sketch(out, h - c + rim - m, &format!("{name} 3"), shapes(c, r_mid));
    let last = if p.open { a + 0.2 } else { a };
    out.push(extrude(&format!("{name} 3"), last, Op::Cut(body), Some(-45.0), "negative"));
}

/// Round corners of `body`: cut away what lies outside a `PLATE_R` arc at each corner given
/// (whether to, its point, and which way is inside along X and Y).
fn corners(out: &mut Vec<Value>, name: &str, body: &str, h: f64, list: &[(bool, [f64; 2], [f64; 2])]) {
    let r = PLATE_R;
    let mut items = Vec::new();
    let mut picks = Vec::new();
    for &(on, [x, y], [sx, sy]) in list {
        if !on {
            continue;
        }
        items.push(cmd("sketch.rectangle.two_point", json!({"p0": [r4(x), r4(y)], "p1": [r4(x + sx * r), r4(y + sy * r)]})));
        items.push(circle(x + sx * r, y + sy * r, 2.0 * r));
        // a point in the corner, outside the arc
        picks.push(json!({"point": [r4(x + sx * 0.3), r4(y + sy * 0.3)]}));
    }
    if items.is_empty() {
        return;
    }
    sketch(out, 0.0, name, items);
    let mut e = extrude(name, h, Op::Cut(body), None, "");
    e["params"]["profiles"] = json!(picks);
    out.push(e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{baseplate_piece, BaseplateOptions};

    fn plate(magnets: bool, screws: bool, pitch: f64) -> Vec<Value> {
        let o = BaseplateOptions { nx: 2, ny: 3, pitch: [pitch; 2], open: false, magnets, screws, filler: [0.0; 4],
            half_cells: false, origin: [0.0, 0.0], name: "P".into() };
        let mut out = Vec::new();
        piece_commands(&mut out, &baseplate_piece(&o));
        out
    }

    /// The circles drawn in sketch `name`, as (x, y, diameter), and the extrude that uses it.
    fn holes(c: &[Value], name: &str) -> (Vec<(f64, f64, f64)>, Option<Value>) {
        let start = c.iter().position(|v| v["command"] == "sketch.create" && v["params"]["name"] == name);
        let Some(start) = start else { return (Vec::new(), None) };
        let circles = c[start..].iter().take_while(|v| v["command"] != "sketch.finish")
            .filter(|v| v["command"] == "sketch.circle.center")
            .map(|v| (v["params"]["center"][0].as_f64().unwrap(), v["params"]["center"][1].as_f64().unwrap(),
                v["params"]["diameter"].as_f64().unwrap()))
            .collect();
        let e = c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"] == name).cloned();
        (circles, e)
    }

    #[test]
    fn a_floor_holds_the_countersink_and_the_magnet_above_it() {
        let depth = PLATE_PROFILE.0 + PLATE_PROFILE.1 + PLATE_PROFILE.2;
        assert!((plate_height(false, false, true) - plate_height(false, true, false)).abs() < 1e-9);
        let floor = plate_height(false, true, true) - depth + PLATE_RIM;
        // under the magnet hole: the countersink, then 0.6 mm for the magnet to rest on
        assert!((floor - MAGNET_DEPTH - SCREW_SINK - 0.65).abs() < 1e-9);
        assert!((plate_height(true, false, true) - plate_height(true, false, false)).abs() < 1e-9);
        assert!((SCREW_SINK - 1.7).abs() < 1e-9);
    }

    #[test]
    fn screw_holes_go_where_the_magnets_do() {
        for pitch in [PITCH, HALF_PITCH] {
            let c = plate(true, true, pitch);
            let (mags, _) = holes(&c, "P Magnets");
            let (screws, hole) = holes(&c, "P Screws");
            let (sinks, sink) = holes(&c, "P Screw countersinks");
            let at = |v: &[(f64, f64, f64)]| v.iter().map(|&(x, y, _)| (x, y)).collect::<Vec<_>>();
            assert!(!mags.is_empty());
            assert_eq!(at(&mags), at(&screws));
            assert_eq!(at(&mags), at(&sinks));
            assert!(screws.iter().all(|h| h.2 == PLATE_SCREW_D) && sinks.iter().all(|h| h.2 == PLATE_SCREW_HEAD_D));
            let (hole, sink) = (hole.unwrap(), sink.unwrap());
            // both cut up from the underside, the countersink narrowing as it goes
            assert_eq!((hole["params"]["direction"].clone(), sink["params"]["direction"].clone()), (json!("positive"), json!("positive")));
            assert_eq!(sink["params"]["taper"], json!(-45.0));
            assert_eq!(hole["params"]["operation"], json!("cut"));
        }
        // and without magnets in the same spots
        let (with, _) = holes(&plate(true, true, PITCH), "P Screws");
        let (without, _) = holes(&plate(false, true, PITCH), "P Screws");
        assert_eq!(with, without);
        assert!(holes(&plate(false, true, PITCH), "P Magnets").0.is_empty());
        assert!(holes(&plate(true, false, PITCH), "P Screws").0.is_empty());
    }
}
