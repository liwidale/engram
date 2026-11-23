//! Directed dependency graph over architecture nodes (`A -> B` means A depends on B).

use crate::schema::{ArchNode, Architecture, NodeKind, Point};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct Graph<'a> {
    nodes: &'a [ArchNode],
    index: HashMap<&'a str, usize>,
    out: Vec<Vec<usize>>,
    inc: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MissingEdge {
    pub from: String,
    pub to: String,
}

impl<'a> Graph<'a> {
    pub fn new(arch: &'a Architecture) -> Self {
        let nodes = arch.nodes.as_slice();
        let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
        let mut out = vec![Vec::new(); nodes.len()];
        let mut inc = vec![Vec::new(); nodes.len()];
        for (i, n) in nodes.iter().enumerate() {
            let mut seen = HashSet::new();
            for dep in &n.depends_on {
                if let Some(&j) = index.get(dep.as_str()) {
                    if j != i && seen.insert(j) {
                        out[i].push(j);
                        inc[j].push(i);
                    }
                }
            }
        }
        Self { nodes, index, out, inc }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    fn ids(&self, ix: &[usize]) -> Vec<String> {
        ix.iter().map(|&i| self.nodes[i].id.clone()).collect()
    }

    pub fn dependencies(&self, id: &str) -> Vec<String> {
        self.index.get(id).map(|&i| self.ids(&self.out[i])).unwrap_or_default()
    }

    pub fn dependents(&self, id: &str) -> Vec<String> {
        self.index.get(id).map(|&i| self.ids(&self.inc[i])).unwrap_or_default()
    }

    /// Edges whose target does not exist (zombie references).
    pub fn missing_edges(&self) -> Vec<MissingEdge> {
        self.nodes
            .iter()
            .flat_map(|n| {
                n.depends_on
                    .iter()
                    .filter(|d| !self.index.contains_key(d.as_str()))
                    .map(|d| MissingEdge { from: n.id.clone(), to: d.clone() })
            })
            .collect()
    }

    /// Everything `id` depends on, directly or transitively, in BFS order.
    pub fn transitive_dependencies(&self, id: &str) -> Vec<String> {
        let Some(&start) = self.index.get(id) else { return Vec::new() };
        let mut seen = vec![false; self.len()];
        seen[start] = true;
        let mut queue = VecDeque::from([start]);
        let mut order = Vec::new();
        while let Some(i) = queue.pop_front() {
            for &j in &self.out[i] {
                if !std::mem::replace(&mut seen[j], true) {
                    order.push(j);
                    queue.push_back(j);
                }
            }
        }
        self.ids(&order)
    }

    /// Undirected hop distance from any seed, up to `max_depth`.
    pub fn neighborhood(&self, seeds: &[&str], max_depth: usize) -> HashMap<String, usize> {
        let mut dist: HashMap<usize, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        for s in seeds {
            if let Some(&i) = self.index.get(s) {
                if dist.insert(i, 0).is_none() {
                    queue.push_back(i);
                }
            }
        }
        while let Some(i) = queue.pop_front() {
            let d = dist[&i];
            if d == max_depth {
                continue;
            }
            for &j in self.out[i].iter().chain(&self.inc[i]) {
                if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(j) {
                    e.insert(d + 1);
                    queue.push_back(j);
                }
            }
        }
        dist.into_iter().map(|(i, d)| (self.nodes[i].id.clone(), d)).collect()
    }

    /// Strongly connected components (Tarjan), in reverse topological order.
    fn sccs(&self) -> Vec<Vec<usize>> {
        struct St<'g> {
            g: &'g [Vec<usize>],
            idx: Vec<Option<usize>>,
            low: Vec<usize>,
            on: Vec<bool>,
            stack: Vec<usize>,
            next: usize,
            out: Vec<Vec<usize>>,
        }
        fn visit(s: &mut St, v: usize) {
            s.idx[v] = Some(s.next);
            s.low[v] = s.next;
            s.next += 1;
            s.stack.push(v);
            s.on[v] = true;
            for k in 0..s.g[v].len() {
                let w = s.g[v][k];
                match s.idx[w] {
                    None => {
                        visit(s, w);
                        s.low[v] = s.low[v].min(s.low[w]);
                    }
                    Some(iw) if s.on[w] => s.low[v] = s.low[v].min(iw),
                    _ => {}
                }
            }
            if Some(s.low[v]) == s.idx[v] {
                let mut comp = Vec::new();
                loop {
                    let w = s.stack.pop().expect("tarjan stack");
                    s.on[w] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                comp.sort_unstable();
                s.out.push(comp);
            }
        }
        let n = self.len();
        let mut s = St {
            g: &self.out,
            idx: vec![None; n],
            low: vec![0; n],
            on: vec![false; n],
            stack: Vec::new(),
            next: 0,
            out: Vec::new(),
        };
        for v in 0..n {
            if s.idx[v].is_none() {
                visit(&mut s, v);
            }
        }
        s.out
    }

    /// Dependency cycles (each a set of node ids, sorted by declaration order).
    pub fn cycles(&self) -> Vec<Vec<String>> {
        let mut cycles: Vec<Vec<String>> = self.sccs().into_iter().filter(|c| c.len() > 1).map(|c| self.ids(&c)).collect();
        cycles.sort();
        cycles
    }

    /// Build order (dependencies before dependents), or `None` if the graph has a cycle.
    pub fn topological_order(&self) -> Option<Vec<String>> {
        let mut remaining: Vec<usize> = self.out.iter().map(Vec::len).collect();
        let mut queue: VecDeque<usize> = (0..self.len()).filter(|&i| remaining[i] == 0).collect();
        let mut order = Vec::with_capacity(self.len());
        while let Some(i) = queue.pop_front() {
            order.push(i);
            for &d in &self.inc[i] {
                remaining[d] -= 1;
                if remaining[d] == 0 {
                    queue.push_back(d);
                }
            }
        }
        (order.len() == self.len()).then(|| self.ids(&order))
    }

    /// Layered (Sugiyama-style) layout: dependents above their dependencies, cycles collapsed
    /// into a shared layer, crossings reduced with barycenter sweeps. Manual positions win.
    pub fn layout(&self, opts: LayoutOptions) -> Layout {
        let n = self.len();
        let comps = self.sccs();
        let mut comp_of = vec![0; n];
        for (c, members) in comps.iter().enumerate() {
            for &v in members {
                comp_of[v] = c;
            }
        }
        // Tarjan emits components in reverse topological order (sinks first), so a single
        // pass in reverse assigns each component its longest distance from any root.
        let mut comp_layer = vec![0usize; comps.len()];
        for c in (0..comps.len()).rev() {
            for &v in &comps[c] {
                for &w in &self.out[v] {
                    let cw = comp_of[w];
                    if cw != c {
                        comp_layer[cw] = comp_layer[cw].max(comp_layer[c] + 1);
                    }
                }
            }
        }
        let layer_of: Vec<usize> = (0..n).map(|v| comp_layer[comp_of[v]]).collect();
        let layer_count = layer_of.iter().copied().max().map_or(0, |m| m + 1);
        let mut layers: Vec<Vec<usize>> = vec![Vec::new(); layer_count];
        for v in 0..n {
            layers[layer_of[v]].push(v);
        }

        let mut pos = vec![0.0f64; n];
        let reindex = |layers: &Vec<Vec<usize>>, pos: &mut Vec<f64>| {
            for layer in layers {
                for (k, &v) in layer.iter().enumerate() {
                    pos[v] = k as f64;
                }
            }
        };
        reindex(&layers, &mut pos);
        for sweep in 0..opts.sweeps {
            let down = sweep % 2 == 0;
            let order: Vec<usize> = if down { (1..layer_count).collect() } else { (0..layer_count.saturating_sub(1)).rev().collect() };
            for l in order {
                let neighbors = |v: usize| if down { &self.inc[v] } else { &self.out[v] };
                let mut keyed: Vec<(f64, usize)> = layers[l]
                    .iter()
                    .map(|&v| {
                        let ns = neighbors(v);
                        let key = if ns.is_empty() { pos[v] } else { ns.iter().map(|&u| pos[u]).sum::<f64>() / ns.len() as f64 };
                        (key, v)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                layers[l] = keyed.into_iter().map(|(_, v)| v).collect();
                reindex(&layers, &mut pos);
            }
        }

        let widest = layers.iter().map(Vec::len).max().unwrap_or(0) as f64;
        let mut placed = Vec::with_capacity(n);
        for (l, layer) in layers.iter().enumerate() {
            let offset = (widest - layer.len() as f64) * opts.column_spacing / 2.0;
            for (k, &v) in layer.iter().enumerate() {
                let node = &self.nodes[v];
                let auto = Point {
                    x: opts.margin + offset + k as f64 * opts.column_spacing,
                    y: opts.margin + l as f64 * opts.row_spacing,
                };
                placed.push(PlacedNode {
                    id: node.id.clone(),
                    label: node.display_name().to_owned(),
                    kind: node.kind,
                    layer: l,
                    position: node.position.unwrap_or(auto),
                    manual: node.position.is_some(),
                });
            }
        }
        placed.sort_by_key(|p| self.index[p.id.as_str()]);

        let cyclic: HashSet<(usize, usize)> = comps
            .iter()
            .filter(|c| c.len() > 1)
            .flat_map(|c| c.iter().flat_map(|&v| self.out[v].iter().filter(|w| c.contains(w)).map(move |&w| (v, w))))
            .collect();
        let mut edges: Vec<LayoutEdge> = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            for &j in &self.out[i] {
                edges.push(LayoutEdge { from: node.id.clone(), to: self.nodes[j].id.clone(), in_cycle: cyclic.contains(&(i, j)) });
            }
        }
        let (mut w, mut h) = (0.0f64, 0.0f64);
        for p in &placed {
            w = w.max(p.position.x + opts.column_spacing / 2.0 + opts.margin);
            h = h.max(p.position.y + opts.row_spacing / 2.0 + opts.margin);
        }
        Layout { nodes: placed, edges, missing: self.missing_edges(), cycles: self.cycles(), width: w, height: h }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutOptions {
    pub column_spacing: f64,
    pub row_spacing: f64,
    pub margin: f64,
    pub sweeps: usize,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self { column_spacing: 220.0, row_spacing: 140.0, margin: 120.0, sweeps: 8 }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PlacedNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: NodeKind,
    pub layer: usize,
    pub position: Point,
    pub manual: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LayoutEdge {
    pub from: String,
    pub to: String,
    pub in_cycle: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Layout {
    pub nodes: Vec<PlacedNode>,
    pub edges: Vec<LayoutEdge>,
    pub missing: Vec<MissingEdge>,
    pub cycles: Vec<Vec<String>>,
    pub width: f64,
    pub height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Text;

    fn arch(edges: &[(&str, &[&str])]) -> Architecture {
        Architecture {
            nodes: edges
                .iter()
                .map(|(id, deps)| ArchNode {
                    id: (*id).into(),
                    kind: NodeKind::Module,
                    label: None,
                    description: Text::default(),
                    path: None,
                    depends_on: deps.iter().map(|d| (*d).into()).collect(),
                    position: None,
                })
                .collect(),
        }
    }

    #[test]
    fn queries_and_topological_order() {
        let a = arch(&[("ui", &["api"]), ("api", &["db", "auth"]), ("auth", &["db"]), ("db", &[]), ("ghost-user", &["nope"])]);
        let g = Graph::new(&a);
        assert_eq!(g.dependencies("api"), vec!["db", "auth"]);
        assert_eq!(g.dependents("db"), vec!["api", "auth"]);
        assert_eq!(g.transitive_dependencies("ui"), vec!["api", "db", "auth"]);
        assert_eq!(g.missing_edges(), vec![MissingEdge { from: "ghost-user".into(), to: "nope".into() }]);
        let order = g.topological_order().unwrap();
        let at = |id: &str| order.iter().position(|x| x == id).unwrap();
        assert!(at("db") < at("auth") && at("auth") < at("api") && at("api") < at("ui"));
        assert!(g.cycles().is_empty());
        let hood = g.neighborhood(&["auth"], 1);
        assert_eq!(hood.get("auth"), Some(&0));
        assert_eq!(hood.get("api"), Some(&1));
        assert_eq!(hood.get("ui"), None);
    }

    #[test]
    fn detects_cycles_and_layouts_them() {
        let a = arch(&[("a", &["b"]), ("b", &["c"]), ("c", &["a", "d"]), ("d", &[])]);
        let g = Graph::new(&a);
        assert_eq!(g.cycles(), vec![vec!["a".to_string(), "b".into(), "c".into()]]);
        assert!(g.topological_order().is_none());
        let layout = g.layout(LayoutOptions::default());
        let layer = |id: &str| layout.nodes.iter().find(|n| n.id == id).unwrap().layer;
        assert_eq!(layer("a"), layer("b"));
        assert!(layer("d") > layer("c"));
        assert_eq!(layout.edges.iter().filter(|e| e.in_cycle).count(), 3);
    }

    #[test]
    fn layout_places_dependents_above_dependencies_and_keeps_manual_positions() {
        let mut a = arch(&[("ui", &["api"]), ("api", &["db"]), ("db", &[]), ("cli", &["api"])]);
        a.nodes[3].position = Some(Point { x: 5.0, y: 6.0 });
        let layout = Graph::new(&a).layout(LayoutOptions::default());
        let get = |id: &str| layout.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(get("ui").position.y < get("api").position.y);
        assert!(get("api").position.y < get("db").position.y);
        assert_eq!(get("cli").position, Point { x: 5.0, y: 6.0 });
        assert!(get("cli").manual);
        assert_eq!(layout.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["ui", "api", "db", "cli"]);
    }
}
