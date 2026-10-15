//! Safe wrapper over the Clipper2 union shim (`third_party/clipper2/shim.cpp`).

use std::ffi::c_void;

use super::{Point, Ring};

unsafe extern "C" {
    fn bb_clipper_union(xy: *const i64, lens: *const usize, count: usize) -> *mut c_void;
    fn bb_clipper_node_count(result: *const c_void) -> usize;
    fn bb_clipper_node(
        result: *const c_void,
        index: usize,
        parent: *mut isize,
        hole: *mut i32,
        xy: *mut *const i64,
    ) -> usize;
    fn bb_clipper_free(result: *mut c_void);
}

struct Node {
    parent: Option<usize>,
    hole: bool,
    points: Ring,
    children: Vec<usize>,
}

/// A Clipper2 `PolyTree64`, flattened in pre-order.
pub(super) struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    pub fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.nodes.len()).filter(|&i| self.nodes[i].parent.is_none())
    }

    pub fn children(&self, node: usize) -> &[usize] {
        &self.nodes[node].children
    }

    pub fn is_hole(&self, node: usize) -> bool {
        self.nodes[node].hole
    }

    pub fn points(&self, node: usize) -> &[Point] {
        &self.nodes[node].points
    }
}

/// NonZero union of `paths` (no clip paths), exactly as KiCad's `booleanOp`.
pub(super) fn union(paths: &[Ring]) -> Tree {
    let xy: Vec<i64> = paths.iter().flatten().flat_map(|&(x, y)| [x, y]).collect();
    let lens: Vec<usize> = paths.iter().map(Vec::len).collect();
    // SAFETY: `xy`/`lens` describe `paths.len()` paths whose point counts sum
    // to `xy.len() / 2`. The result handle is read while alive and freed once.
    unsafe {
        let result = bb_clipper_union(xy.as_ptr(), lens.as_ptr(), lens.len());
        let mut nodes: Vec<Node> = (0..bb_clipper_node_count(result))
            .map(|index| {
                let (mut parent, mut hole, mut data) = (0isize, 0i32, std::ptr::null());
                let count = bb_clipper_node(result, index, &mut parent, &mut hole, &mut data);
                let flat = if count == 0 { &[][..] } else { std::slice::from_raw_parts(data, count * 2) };
                Node {
                    parent: usize::try_from(parent).ok(),
                    hole: hole != 0,
                    points: flat.chunks_exact(2).map(|p| (p[0], p[1])).collect(),
                    children: Vec::new(),
                }
            })
            .collect();
        bb_clipper_free(result);
        for i in 0..nodes.len() {
            if let Some(parent) = nodes[i].parent {
                nodes[parent].children.push(i);
            }
        }
        Tree { nodes }
    }
}
