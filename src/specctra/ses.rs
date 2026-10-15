//! `ImportSpecctraSES`: merge a FreeRouting session back into a board.
//!
//! Semantics follow KiCad 10's `SPECCTRA_DB::FromSESSION`: every unlocked
//! track, arc, and via is removed (locked ones were exported as `fix` and do
//! not come back), footprints are moved to the session placement, and the
//! session's wires and vias become new `segment` and `via` items.

use anyhow::{Context, Result, anyhow, bail};

use super::board::{ProjectRules, mm};
use super::geom::{P, kiround, normalize_deg};
use crate::sexpr::Sexp;

/// What a session import changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportStats {
    /// Unlocked tracks, arcs, and vias removed from the board.
    pub removed: usize,
    pub segments: usize,
    pub vias: usize,
    /// Footprints whose position or orientation the session changed.
    pub moved: usize,
}

// ------------------------------------------------------------------ lexer --

/// A Specctra token tree. DSN quoting differs from KiCad's s-expressions
/// (e.g. `(string_quote ")`), so sessions get their own small reader.
#[derive(Clone, Debug)]
enum Node {
    Word(String),
    List(Vec<Node>),
}

impl Node {
    fn head(&self) -> Option<&str> {
        match self {
            Node::List(items) => match items.first() {
                Some(Node::Word(w)) => Some(w),
                _ => None,
            },
            Node::Word(_) => None,
        }
    }

    fn items(&self) -> &[Node] {
        match self {
            Node::List(items) => items.get(1..).unwrap_or(&[]),
            Node::Word(_) => &[],
        }
    }

    fn child(&self, head: &str) -> Option<&Node> {
        self.items().iter().find(|n| n.head() == Some(head))
    }

    fn children<'a>(&'a self, head: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.items().iter().filter(move |n| n.head() == Some(head))
    }

    fn word(&self, index: usize) -> Option<&str> {
        match self.items().get(index)? {
            Node::Word(w) => Some(w),
            Node::List(_) => None,
        }
    }

    fn words(&self) -> Vec<&str> {
        self.items()
            .iter()
            .filter_map(|n| match n {
                Node::Word(w) => Some(w.as_str()),
                Node::List(_) => None,
            })
            .collect()
    }
}

fn parse_dsn(text: &str) -> Result<Node> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let mut quote = '"';
    let node = parse_node(&chars, &mut pos, &mut quote)?;
    Ok(node)
}

fn parse_node(c: &[char], pos: &mut usize, quote: &mut char) -> Result<Node> {
    skip_ws(c, pos);
    match c.get(*pos) {
        None => bail!("unexpected end of session file"),
        Some('(') => {
            *pos += 1;
            let mut items = Vec::new();
            loop {
                skip_ws(c, pos);
                match c.get(*pos) {
                    None => bail!("unbalanced '(' in session file"),
                    Some(')') => {
                        *pos += 1;
                        return Ok(Node::List(items));
                    }
                    _ => {
                        let after_string_quote = matches!(items.as_slice(), [Node::Word(w)] if w == "string_quote");
                        if after_string_quote {
                            skip_ws(c, pos);
                            let q = *c.get(*pos).context("missing quote character")?;
                            *quote = q;
                            *pos += 1;
                            items.push(Node::Word(q.to_string()));
                        } else {
                            items.push(parse_node(c, pos, quote)?);
                        }
                    }
                }
            }
        }
        Some(')') => bail!("unbalanced ')' in session file"),
        Some(&ch) if ch == *quote => {
            *pos += 1;
            let start = *pos;
            while c.get(*pos).is_some_and(|&x| x != *quote) {
                *pos += 1;
            }
            let word: String = c[start..*pos].iter().collect();
            *pos += 1;
            Ok(Node::Word(word))
        }
        Some(_) => {
            let start = *pos;
            while c.get(*pos).is_some_and(|&x| !x.is_whitespace() && x != '(' && x != ')') {
                *pos += 1;
            }
            Ok(Node::Word(c[start..*pos].iter().collect()))
        }
    }
}

fn skip_ws(c: &[char], pos: &mut usize) {
    while c.get(*pos).is_some_and(|x| x.is_whitespace()) {
        *pos += 1;
    }
}

// ----------------------------------------------------------------- import --

/// `(resolution um 10)` as the nanometres per session unit.
#[derive(Clone, Copy)]
struct Resolution {
    factor: f64,
    value: f64,
}

impl Resolution {
    fn of(node: Option<&Node>) -> Self {
        let Some(node) = node else { return Self { factor: 25.4e6, value: 2_540_000.0 } };
        let factor = match node.word(0).unwrap_or("inch") {
            "mil" => 25.4e3,
            "cm" => 1e7,
            "mm" => 1e6,
            "um" => 1e3,
            _ => 25.4e6,
        };
        let value = node.word(1).and_then(|v| v.parse().ok()).unwrap_or(1.0);
        Self { factor, value }
    }

    fn scale(&self, distance: f64) -> i64 {
        kiround(self.factor * distance / self.value)
    }

    fn point(&self, x: f64, y: f64) -> P {
        P::new(self.scale(x), -self.scale(y))
    }
}

fn number(text: &str) -> Result<f64> {
    text.parse().map_err(|_| anyhow!("bad number {text:?} in session file"))
}

fn mm_atom(nm: i64) -> Sexp {
    Sexp::atom(crate::units::Nm(nm).to_string())
}

fn xy(head: &str, p: P) -> Sexp {
    Sexp::node(head, [mm_atom(p.x), mm_atom(p.y)])
}

fn uuid() -> Sexp {
    Sexp::node("uuid", [Sexp::str(uuid::Uuid::new_v4().to_string())])
}

fn is_track(node: &Sexp) -> bool {
    matches!(node.head(), Some("segment" | "arc" | "via"))
}

fn is_locked(node: &Sexp) -> bool {
    node.args().iter().any(|a| matches!(a, Sexp::Atom(t) if t == "locked")) || node.value("locked") == Some("yes")
}

fn track_net(node: &Sexp) -> Option<&str> {
    node.child("net").and_then(|n| n.arg(n.args().len().saturating_sub(1))).and_then(Sexp::text)
}

/// Import `ses` into `board` in place. `rules` supplies the net classes
/// whose default via drill pcbnew treats as "use the net class drill".
pub fn import_ses(board: &mut Sexp, ses: &str, rules: &ProjectRules) -> Result<ImportStats> {
    let session = parse_dsn(ses)?;
    if session.head() != Some("session") {
        bail!("Session file is missing the \"session\" section");
    }
    let routes = session.child("routes").context("Session file is missing the \"routes\" section")?;
    let library = routes.child("library_out").context("Session file is missing the \"library_out\" section")?;
    let resolution = Resolution::of(routes.child("resolution"));

    let copper: Vec<String> = board
        .child("layers")
        .map(|layers| {
            let mut found: Vec<(i64, String)> = layers
                .args()
                .iter()
                .filter_map(|l| {
                    let id = l.head()?.parse::<i64>().ok()?;
                    let name = l.arg(0)?.text()?.to_owned();
                    name.ends_with(".Cu").then_some((id, name))
                })
                .collect();
            found.sort_by_key(|(id, _)| {
                if *id == 0 {
                    -1
                } else if *id == 2 {
                    i64::MAX
                } else {
                    *id
                }
            });
            found.into_iter().map(|(_, n)| n).collect()
        })
        .unwrap_or_default();
    let known_nets: std::collections::HashSet<String> = {
        let mut names = std::collections::HashSet::new();
        collect_net_names(board, &mut names);
        names
    };

    let items = board.as_list_mut().context("board is not a list")?;
    let before = items.len();
    items.retain(|item| !is_track(item) || is_locked(item));
    let mut stats = ImportStats { removed: before - items.len(), ..ImportStats::default() };

    if let Some(placement) = session.child("placement") {
        let place_res = Resolution::of(placement.child("resolution"));
        for component in placement.children("component") {
            for place in component.children("place") {
                let words = place.words();
                let reference = *words.first().context("place without reference")?;
                if words.len() < 5 {
                    continue;
                }
                let position = place_res.point(number(words[1])?, number(words[2])?);
                let side = words[3];
                let rotation = number(words[4])?;
                if side != "front" {
                    bail!("{reference}: back-side placement in session is not supported");
                }
                if move_footprint(items, reference, position, rotation)? {
                    stats.moved += 1;
                }
            }
        }
    }

    let default_drill = rules.default.via_drill;
    for net_out in routes.child("network_out").into_iter().flat_map(|n| n.children("net")) {
        let net_name = net_out.word(0).unwrap_or_default();
        let net = if known_nets.contains(net_name) { net_name } else { "" };
        for wire in net_out.children("wire") {
            let fixed = wire.child("type").and_then(|t| t.word(0)) == Some("fix");
            let Some(path) = wire.child("path") else { continue };
            let words = path.words();
            let layer = words.first().copied().context("wire path without layer")?;
            if !copper.iter().any(|l| l == layer) {
                bail!("Session file uses invalid layer id '{layer}'.");
            }
            let width = resolution.scale(number(words.get(1).copied().unwrap_or("0"))?);
            let coords: Vec<f64> = words.iter().skip(2).map(|w| number(w)).collect::<Result<_>>()?;
            let points: Vec<P> = coords.chunks_exact(2).map(|xy| resolution.point(xy[0], xy[1])).collect();
            for pair in points.windows(2) {
                let mut seg = vec![
                    Sexp::atom("segment"),
                    xy("start", pair[0]),
                    xy("end", pair[1]),
                    Sexp::node("width", [mm_atom(width)]),
                    Sexp::node("layer", [Sexp::str(layer)]),
                ];
                if fixed {
                    seg.push(Sexp::node("locked", [Sexp::atom("yes")]));
                }
                seg.push(Sexp::node("net", [Sexp::str(net)]));
                seg.push(uuid());
                items.push(Sexp::List(seg));
                stats.segments += 1;
            }
        }
        for wire_via in net_out.children("via") {
            let words = wire_via.words();
            let padstack_id = *words.first().context("via without padstack")?;
            let padstack = library
                .children("padstack")
                .find(|p| p.word(0) == Some(padstack_id))
                .with_context(|| format!("A wire_via refers to missing padstack '{padstack_id}'."))?;
            let shapes: Vec<&Node> = padstack.children("shape").collect();
            let circles: Vec<(&str, f64)> = shapes
                .iter()
                .map(|s| {
                    let circle = s.child("circle").context("Unsupported via shape")?;
                    Ok((circle.word(0).unwrap_or_default(), number(circle.word(1).unwrap_or("0"))?))
                })
                .collect::<Result<_>>()?;
            if circles.is_empty() {
                bail!("Session via padstack has no shapes");
            }
            let diameter = resolution.scale(circles[0].1);
            let (top, bottom) = if circles.len() == 1 || circles.len() == copper.len() {
                (copper.first().cloned().unwrap_or_default(), copper.last().cloned().unwrap_or_default())
            } else {
                let indices: Vec<usize> =
                    circles.iter().filter_map(|(layer, _)| copper.iter().position(|l| l == layer)).collect();
                let (lo, hi) = (*indices.iter().min().unwrap_or(&0), *indices.iter().max().unwrap_or(&0));
                (copper[lo].clone(), copper[hi].clone())
            };
            // pcbnew encodes the drill after ':' in its via names; the default
            // net class drill means "use the via's net class drill".
            let drill = padstack_id
                .split_once(':')
                .and_then(|(_, rest)| rest.rsplit_once('_'))
                .and_then(|(um, _)| um.trim().parse::<f64>().ok())
                .map(|um| (um * (1e6 / 1000.0)) as i64)
                .filter(|&d| d != default_drill)
                .unwrap_or_else(|| rules.class_of(net).via_drill);
            let fixed = wire_via.child("type").and_then(|t| t.word(0)) == Some("fix");
            let coords: Vec<f64> = words.iter().skip(1).map(|w| number(w)).collect::<Result<_>>()?;
            for xy_pair in coords.chunks_exact(2) {
                let at = resolution.point(xy_pair[0], xy_pair[1]);
                let mut via = vec![
                    Sexp::atom("via"),
                    xy("at", at),
                    Sexp::node("size", [mm_atom(diameter)]),
                    Sexp::node("drill", [mm_atom(drill)]),
                    Sexp::node("layers", [Sexp::str(top.clone()), Sexp::str(bottom.clone())]),
                ];
                if fixed {
                    via.push(Sexp::node("locked", [Sexp::atom("yes")]));
                }
                via.push(Sexp::node("net", [Sexp::str(net)]));
                via.push(uuid());
                items.push(Sexp::List(via));
                stats.vias += 1;
            }
        }
    }
    Ok(stats)
}

fn collect_net_names(node: &Sexp, names: &mut std::collections::HashSet<String>) {
    if node.is("filled_polygon") {
        return;
    }
    if node.is("net") {
        if let Some(name) = track_net(&Sexp::List(vec![Sexp::atom("x"), node.clone()])) {
            names.insert(name.to_owned());
        }
        return;
    }
    for child in node.args() {
        if child.as_list().is_some() {
            collect_net_names(child, names);
        }
    }
}

/// Apply a session placement: `SetPosition` and `SetOrientation`. Children
/// are stored footprint-relative, so only the footprint's own `(at)` and the
/// absolute angles of its pads and fields change.
fn move_footprint(items: &mut [Sexp], reference: &str, position: P, rotation: f64) -> Result<bool> {
    let fp = items
        .iter_mut()
        .find(|item| {
            item.is("footprint")
                && item.children("property").any(|p| {
                    p.arg(0).and_then(Sexp::text) == Some("Reference")
                        && p.arg(1).and_then(Sexp::text) == Some(reference)
                })
        })
        .with_context(|| format!("Reference '{reference}' not found."))?;
    let at = fp.child("at").context("footprint without (at)")?;
    let old = P::new(mm(at.arg(0).context("x")?)?, mm(at.arg(1).context("y")?)?);
    let old_angle = at.arg(2).and_then(Sexp::as_f64).unwrap_or(0.0);
    let mut new_angle = normalize_deg(rotation);
    if new_angle > 180.0 {
        new_angle -= 360.0;
    }
    let delta = new_angle - old_angle;
    if old == position && normalize_deg(delta) == 0.0 {
        return Ok(false);
    }
    let at = fp.child_mut("at").context("footprint without (at)")?;
    let mut fields = vec![Sexp::atom("at"), mm_atom(position.x), mm_atom(position.y)];
    if new_angle != 0.0 {
        fields.push(super::geom_angle(new_angle));
    }
    *at = Sexp::List(fields);
    if normalize_deg(delta) != 0.0 {
        for child in fp.as_list_mut().into_iter().flatten() {
            if child.is("pad") || child.is("property") || child.is("fp_text") {
                rotate_child_angle(child, delta);
            }
        }
    }
    Ok(true)
}

fn rotate_child_angle(child: &mut Sexp, delta: f64) {
    let Some(at) = child.child_mut("at") else { return };
    let Some(items) = at.as_list_mut() else { return };
    let angle = items.get(3).and_then(Sexp::as_f64).unwrap_or(0.0);
    let rotated = normalize_deg(angle + delta);
    items.truncate(3);
    items.push(super::geom_angle(rotated));
}
