//! Geometric constraints and a numeric solver.
//!
//! Constraints reference path nodes by object id and index. The solver bakes the
//! object transform, moves node points with a damped least-squares step, and
//! writes the result back as a path. A constraint set that cannot be satisfied
//! returns an error and leaves the document untouched.
//!
//! Segment-based constraints (parallel, perpendicular, tangent) need straight
//! segments; points of curved segments can still take part in coincident,
//! horizontal, vertical, distance and angle constraints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::errors::{BeamError, Result};
use crate::geometry::{Path, Point2, Transform2};

/// One path node in the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRef {
    pub object_id: String,
    pub node: usize,
}

impl NodeRef {
    pub fn new(object_id: impl Into<String>, node: usize) -> Self {
        Self {
            object_id: object_id.into(),
            node,
        }
    }
}

/// What the solver must keep true.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintKind {
    /// Two nodes share a position.
    Coincident { a: NodeRef, b: NodeRef },
    /// Two nodes share the Y coordinate.
    Horizontal { a: NodeRef, b: NodeRef },
    /// Two nodes share the X coordinate.
    Vertical { a: NodeRef, b: NodeRef },
    /// The distance between two nodes is `value_mm`.
    Distance {
        a: NodeRef,
        b: NodeRef,
        value_mm: f64,
    },
    /// The angle at `b`, between `b → a` and `b → c`, is `degrees`.
    Angle {
        a: NodeRef,
        b: NodeRef,
        c: NodeRef,
        degrees: f64,
    },
    /// The segment leaving `a` is parallel to the one leaving `b`.
    Parallel { a: NodeRef, b: NodeRef },
    /// The segment leaving `a` is perpendicular to the one leaving `b`.
    Perpendicular { a: NodeRef, b: NodeRef },
    /// The segment leaving `a` ends where the one leaving `b` starts and the
    /// two directions are collinear.
    Tangent { a: NodeRef, b: NodeRef },
}

impl ConstraintKind {
    pub fn label(&self) -> &'static str {
        match self {
            ConstraintKind::Coincident { .. } => "Coincident",
            ConstraintKind::Horizontal { .. } => "Horizontal",
            ConstraintKind::Vertical { .. } => "Vertical",
            ConstraintKind::Distance { .. } => "Distance",
            ConstraintKind::Angle { .. } => "Angle",
            ConstraintKind::Parallel { .. } => "Parallel",
            ConstraintKind::Perpendicular { .. } => "Perpendicular",
            ConstraintKind::Tangent { .. } => "Tangent",
        }
    }

    /// Stable identifier used by agents and the project file.
    pub fn key(&self) -> &'static str {
        match self {
            ConstraintKind::Coincident { .. } => "coincident",
            ConstraintKind::Horizontal { .. } => "horizontal",
            ConstraintKind::Vertical { .. } => "vertical",
            ConstraintKind::Distance { .. } => "distance",
            ConstraintKind::Angle { .. } => "angle",
            ConstraintKind::Parallel { .. } => "parallel",
            ConstraintKind::Perpendicular { .. } => "perpendicular",
            ConstraintKind::Tangent { .. } => "tangent",
        }
    }

    /// The driving value, for the constraints that have one.
    pub fn value(&self) -> Option<f64> {
        match self {
            ConstraintKind::Distance { value_mm, .. } => Some(*value_mm),
            ConstraintKind::Angle { degrees, .. } => Some(*degrees),
            _ => None,
        }
    }

    /// Set the driving value, if the constraint has one.
    pub fn set_value(&mut self, value: f64) -> bool {
        match self {
            ConstraintKind::Distance { value_mm, .. } => {
                *value_mm = value;
                true
            }
            ConstraintKind::Angle { degrees, .. } => {
                *degrees = value;
                true
            }
            _ => false,
        }
    }

    fn nodes(&self) -> Vec<&NodeRef> {
        match self {
            ConstraintKind::Coincident { a, b }
            | ConstraintKind::Horizontal { a, b }
            | ConstraintKind::Vertical { a, b }
            | ConstraintKind::Distance { a, b, .. }
            | ConstraintKind::Parallel { a, b }
            | ConstraintKind::Perpendicular { a, b }
            | ConstraintKind::Tangent { a, b } => vec![a, b],
            ConstraintKind::Angle { a, b, c, .. } => vec![a, b, c],
        }
    }

    fn uses_segment(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Parallel { .. }
                | ConstraintKind::Perpendicular { .. }
                | ConstraintKind::Tangent { .. }
        )
    }
}

/// One stored constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub kind: ConstraintKind,
    /// Disabled constraints stay in the project but do not drive the solver.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Constraint {
    pub fn new(kind: ConstraintKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            enabled: true,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// A sentence a person can read.
    pub fn describe(&self) -> String {
        match &self.kind {
            ConstraintKind::Coincident { a, b } => {
                format!("{} coincides with {}", short(a), short(b))
            }
            ConstraintKind::Horizontal { a, b } => {
                format!("{} is horizontal with {}", short(a), short(b))
            }
            ConstraintKind::Vertical { a, b } => {
                format!("{} is vertical with {}", short(a), short(b))
            }
            ConstraintKind::Distance { a, b, value_mm } => {
                format!("{} is {value_mm:.2} mm from {}", short(a), short(b))
            }
            ConstraintKind::Angle { a, b, c, degrees } => {
                format!(
                    "angle at {} between {} and {} is {degrees:.1}°",
                    short(b),
                    short(a),
                    short(c)
                )
            }
            ConstraintKind::Parallel { a, b } => {
                format!("segment at {} is parallel to {}", short(a), short(b))
            }
            ConstraintKind::Perpendicular { a, b } => {
                format!("segment at {} is perpendicular to {}", short(a), short(b))
            }
            ConstraintKind::Tangent { a, b } => {
                format!("segment at {} is tangent to {}", short(a), short(b))
            }
        }
    }
}

fn short(node: &NodeRef) -> String {
    format!(
        "{}#{}",
        &node.object_id[..node.object_id.len().min(8)],
        node.node
    )
}

/// Check ids, references and values against a document.
pub fn validate(
    constraints: &[Constraint],
    document: &Document,
) -> std::result::Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    for constraint in constraints {
        if constraint.id.trim().is_empty() {
            return Err("a constraint needs an id".to_string());
        }
        if !ids.insert(constraint.id.as_str()) {
            return Err(format!("duplicate constraint id: {}", constraint.id));
        }
        if let ConstraintKind::Distance { value_mm, .. } = &constraint.kind {
            if !value_mm.is_finite() || *value_mm <= 0.0 {
                return Err("a distance must be positive and finite".to_string());
            }
        }
        if let ConstraintKind::Angle { degrees, .. } = &constraint.kind {
            if !degrees.is_finite() {
                return Err("an angle must be finite".to_string());
            }
        }
        for node in constraint.kind.nodes() {
            validate_node(node, document)?;
        }
        if constraint.kind.uses_segment() {
            for node in constraint.kind.nodes() {
                validate_straight(node, document)?;
            }
        }
    }
    Ok(())
}

fn validate_node(node: &NodeRef, document: &Document) -> std::result::Result<(), String> {
    let object = document
        .object(&node.object_id)
        .ok_or_else(|| format!("constraint references missing object {}", node.object_id))?;
    let path = object.world_path();
    let count = path.nodes().len();
    if node.node >= count {
        return Err(format!(
            "node {} is out of range for '{}' ({} nodes)",
            node.node, object.name, count
        ));
    }
    Ok(())
}

fn validate_straight(node: &NodeRef, document: &Document) -> std::result::Result<(), String> {
    let object = document
        .object(&node.object_id)
        .ok_or_else(|| format!("constraint references missing object {}", node.object_id))?;
    let path = object.world_path();
    let nodes = path.nodes();
    let next = next_index(path.closed, node.node, nodes.len())
        .ok_or_else(|| format!("node {} has no segment leaving it", node.node))?;
    // The world path is flattened from the shape; find the segment through both
    // nodes by comparing endpoints.
    let from = nodes[node.node].point;
    let to = nodes[next].point;
    let mut straight = false;
    for segment in &path.segments {
        if let crate::geometry::path::Segment::Line { from: a, to: b } = segment {
            if (a.distance(from) < 1e-9 && b.distance(to) < 1e-9)
                || (a.distance(to) < 1e-9 && b.distance(from) < 1e-9)
            {
                straight = true;
                break;
            }
        }
    }
    if !straight {
        return Err(format!(
            "node {} of '{}' does not start a straight segment",
            node.node, object.name
        ));
    }
    Ok(())
}

fn next_index(closed: bool, node: usize, count: usize) -> Option<usize> {
    if count < 2 {
        return None;
    }
    if node + 1 < count {
        Some(node + 1)
    } else if closed {
        Some(0)
    } else {
        None
    }
}

/// What the solver did.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Report {
    pub constraints: usize,
    pub iterations: usize,
    pub residual: f64,
    pub moved_nodes: usize,
}

/// Solve every enabled constraint and return the new document.
pub fn solve(document: &Document, ids: Option<&[String]>) -> Result<(Document, Report)> {
    if let Err(error) = validate(&document.constraints, document) {
        return Err(BeamError::InvalidInput(error));
    }
    let selected: Vec<Constraint> = document
        .constraints
        .iter()
        .filter(|constraint| constraint.enabled)
        .filter(|constraint| ids.is_none_or(|ids| ids.iter().any(|id| id == &constraint.id)))
        .cloned()
        .collect();
    if selected.is_empty() {
        return Ok((document.clone(), Report::default()));
    }

    // Working copies in document coordinates, with the transform baked in.
    let mut paths: BTreeMap<String, Path> = BTreeMap::new();
    for constraint in &selected {
        for node in constraint.kind.nodes() {
            paths.entry(node.object_id.clone()).or_insert_with(|| {
                document
                    .object(&node.object_id)
                    .map_or_else(Path::new, crate::document::Object::world_path)
            });
        }
    }
    let mut system = System::new(&selected, &paths)?;
    let mut values = system.initial_values(&paths);
    let solution = system.solve(&mut values)?;

    let mut solved = document.clone();
    let mut moved_nodes = 0usize;
    for (object_id, path) in &paths {
        let mut nodes = path.nodes();
        for (index, node) in nodes.iter_mut().enumerate() {
            let Some(variable) = system.index.get(&(object_id.clone(), index)) else {
                continue;
            };
            let x = solution[variable * 2];
            let y = solution[variable * 2 + 1];
            if (node.point.x - x).abs() > 1e-9 || (node.point.y - y).abs() > 1e-9 {
                let dx = x - node.point.x;
                let dy = y - node.point.y;
                node.point = Point2::new(x, y);
                for handle in [&mut node.handle_in, &mut node.handle_out]
                    .into_iter()
                    .flatten()
                {
                    *handle = Point2::new(handle.x + dx, handle.y + dy);
                }
                moved_nodes += 1;
            }
        }
        if let Some(object) = solved.object_mut(object_id) {
            object.shape = crate::document::Shape::Path(Path::from_nodes(&nodes, path.closed));
            object.transform = Transform2::identity();
        }
    }

    Ok((
        solved,
        Report {
            constraints: selected.len(),
            iterations: system.iterations,
            residual: system.residual,
            moved_nodes,
        },
    ))
}

const MAX_NODES: usize = 200;
const MAX_ITERATIONS: usize = 200;
const TOLERANCE: f64 = 1e-6;

struct System<'a> {
    constraints: &'a [Constraint],
    index: BTreeMap<(String, usize), usize>,
    counts: BTreeMap<String, usize>,
    closed: BTreeMap<String, bool>,
    iterations: usize,
    residual: f64,
}

impl<'a> System<'a> {
    fn new(constraints: &'a [Constraint], paths: &BTreeMap<String, Path>) -> Result<Self> {
        let mut keys: Vec<(String, usize)> = Vec::new();
        for constraint in constraints {
            for node in constraint.kind.nodes() {
                let count = paths
                    .get(&node.object_id)
                    .map_or(0, |path| path.nodes().len());
                let closed = paths.get(&node.object_id).is_some_and(|path| path.closed);
                if node.node >= count {
                    return Err(BeamError::InvalidInput(format!(
                        "node {} is out of range",
                        node.node
                    )));
                }
                let mut involved = vec![(node.object_id.clone(), node.node)];
                if constraint.kind.uses_segment() {
                    let next = next_index(closed, node.node, count).ok_or_else(|| {
                        BeamError::InvalidInput(format!(
                            "node {} has no segment leaving it",
                            node.node
                        ))
                    })?;
                    involved.push((node.object_id.clone(), next));
                }
                for key in involved {
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
        }
        keys.sort();
        if keys.len() > MAX_NODES {
            return Err(BeamError::InvalidInput(format!(
                "a solve is limited to {MAX_NODES} nodes"
            )));
        }
        let index: BTreeMap<(String, usize), usize> = keys
            .into_iter()
            .enumerate()
            .map(|(position, key)| (key, position))
            .collect();
        let counts: BTreeMap<String, usize> = paths
            .iter()
            .map(|(id, path)| (id.clone(), path.nodes().len()))
            .collect();
        let closed: BTreeMap<String, bool> = paths
            .iter()
            .map(|(id, path)| (id.clone(), path.closed))
            .collect();
        Ok(Self {
            constraints,
            index,
            counts,
            closed,
            iterations: 0,
            residual: f64::INFINITY,
        })
    }

    fn initial_values(&self, paths: &BTreeMap<String, Path>) -> Vec<f64> {
        let mut values = vec![0.0; self.index.len() * 2];
        for ((object_id, node), variable) in &self.index {
            let Some(point) = paths
                .get(object_id)
                .and_then(|path| path.nodes().get(*node).map(|node| node.point))
            else {
                continue;
            };
            values[variable * 2] = point.x;
            values[variable * 2 + 1] = point.y;
        }
        values
    }

    fn point(&self, x: &[f64], node: &NodeRef) -> [f64; 2] {
        let Some(variable) = self.index.get(&(node.object_id.clone(), node.node)) else {
            return [0.0, 0.0];
        };
        [x[variable * 2], x[variable * 2 + 1]]
    }

    fn direction(&self, x: &[f64], node: &NodeRef) -> [f64; 2] {
        let count = self.counts.get(&node.object_id).copied().unwrap_or(0);
        let closed = self.closed.get(&node.object_id).copied().unwrap_or(false);
        let Some(next) = next_index(closed, node.node, count) else {
            return [0.0, 0.0];
        };
        let from = self.point(x, node);
        let to = self.point(
            x,
            &NodeRef {
                object_id: node.object_id.clone(),
                node: next,
            },
        );
        [to[0] - from[0], to[1] - from[1]]
    }

    fn residuals(&self, x: &[f64]) -> Vec<f64> {
        let mut out = Vec::new();
        for constraint in self.constraints {
            self.constraint_residuals(&constraint.kind, x, &mut out);
        }
        out
    }

    fn constraint_residuals(&self, kind: &ConstraintKind, x: &[f64], out: &mut Vec<f64>) {
        match kind {
            ConstraintKind::Coincident { a, b } => {
                let (pa, pb) = (self.point(x, a), self.point(x, b));
                out.push(pa[0] - pb[0]);
                out.push(pa[1] - pb[1]);
            }
            ConstraintKind::Horizontal { a, b } => {
                out.push(self.point(x, a)[1] - self.point(x, b)[1]);
            }
            ConstraintKind::Vertical { a, b } => {
                out.push(self.point(x, a)[0] - self.point(x, b)[0]);
            }
            ConstraintKind::Distance { a, b, value_mm } => {
                let (pa, pb) = (self.point(x, a), self.point(x, b));
                out.push((pa[0] - pb[0]).hypot(pa[1] - pb[1]) - value_mm);
            }
            ConstraintKind::Angle { a, b, c, degrees } => {
                let vertex = self.point(x, b);
                let u = subtract(self.point(x, a), vertex);
                let v = subtract(self.point(x, c), vertex);
                if length(u) < 1e-9 || length(v) < 1e-9 {
                    out.push(1e3);
                    return;
                }
                let angle = f64::atan2(cross(u, v), dot(u, v));
                let target = degrees.to_radians();
                out.push(wrap_angle(angle - target));
            }
            ConstraintKind::Parallel { a, b } => {
                out.push(normalized_cross(self.direction(x, a), self.direction(x, b)));
            }
            ConstraintKind::Perpendicular { a, b } => {
                out.push(normalized_dot(self.direction(x, a), self.direction(x, b)));
            }
            ConstraintKind::Tangent { a, b } => {
                let count = self.counts.get(&a.object_id).copied().unwrap_or(0);
                let closed = self.closed.get(&a.object_id).copied().unwrap_or(false);
                let end = next_index(closed, a.node, count).unwrap_or(a.node);
                let pb = self.point(
                    x,
                    &NodeRef {
                        object_id: a.object_id.clone(),
                        node: end,
                    },
                );
                let start_b = self.point(x, b);
                out.push(pb[0] - start_b[0]);
                out.push(pb[1] - start_b[1]);
                out.push(normalized_cross(self.direction(x, a), self.direction(x, b)));
            }
        }
    }

    /// Damped least squares: Gauss–Newton steps with Levenberg–Marquardt damping.
    fn solve(&mut self, values: &mut [f64]) -> Result<Vec<f64>> {
        let mut x = values.to_vec();
        let mut residual = self.residuals(&x);
        let mut lambda = 1e-3;
        let mut iterations = 0;
        for step in 1..=MAX_ITERATIONS {
            iterations = step;
            let worst = max_abs(&residual);
            if worst <= TOLERANCE {
                break;
            }
            let jacobian = self.jacobian(&x, residual.len());
            let mut normal = vec![vec![0.0; x.len()]; x.len()];
            let mut gradient = vec![0.0; x.len()];
            for row in 0..residual.len() {
                for column in 0..x.len() {
                    let value = jacobian[row][column];
                    gradient[column] += value * residual[row];
                    for other in 0..x.len() {
                        normal[column][other] += value * jacobian[row][other];
                    }
                }
            }
            let mut accepted = false;
            for _ in 0..24 {
                let mut matrix = normal.clone();
                for (index, row) in matrix.iter_mut().enumerate() {
                    row[index] += lambda * (1.0 + normal[index][index]);
                }
                let mut rhs: Vec<f64> = gradient.iter().map(|value| -value).collect();
                if let Some(delta) = solve_linear(&mut matrix, &mut rhs) {
                    let trial: Vec<f64> = x
                        .iter()
                        .zip(&delta)
                        .map(|(value, change)| value + change)
                        .collect();
                    let trial_residual = self.residuals(&trial);
                    if sum_of_squares(&trial_residual) < sum_of_squares(&residual) {
                        x = trial;
                        residual = trial_residual;
                        lambda = (lambda * 0.3).max(1e-12);
                        accepted = true;
                        break;
                    }
                }
                lambda *= 10.0;
            }
            if !accepted {
                break;
            }
        }
        self.iterations = iterations;
        self.residual = max_abs(&residual);
        if self.residual > TOLERANCE {
            return Err(BeamError::InvalidInput(format!(
                "the constraints do not converge (worst residual {:.4}); check for conflicting dimensions",
                self.residual
            )));
        }
        values.copy_from_slice(&x);
        Ok(x)
    }

    fn jacobian(&self, x: &[f64], rows: usize) -> Vec<Vec<f64>> {
        let mut jacobian = vec![vec![0.0; x.len()]; rows];
        let step = 1e-5;
        for column in 0..x.len() {
            let mut forward = x.to_vec();
            forward[column] += step;
            let mut backward = x.to_vec();
            backward[column] -= step;
            let plus = self.residuals(&forward);
            let minus = self.residuals(&backward);
            for row in 0..rows {
                jacobian[row][column] = (plus[row] - minus[row]) / (2.0 * step);
            }
        }
        jacobian
    }
}

fn subtract(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

fn normalized_cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    let denominator = length(a) * length(b);
    if denominator < 1e-12 {
        1e3
    } else {
        cross(a, b) / denominator
    }
}

fn normalized_dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    let denominator = length(a) * length(b);
    if denominator < 1e-12 {
        1e3
    } else {
        dot(a, b) / denominator
    }
}

fn wrap_angle(angle: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let mut value = angle % two_pi;
    if value > std::f64::consts::PI {
        value -= two_pi;
    }
    if value < -std::f64::consts::PI {
        value += two_pi;
    }
    value
}

fn max_abs(values: &[f64]) -> f64 {
    values
        .iter()
        .fold(0.0_f64, |worst, value| worst.max(value.abs()))
}

fn sum_of_squares(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

/// Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)]
fn solve_linear(matrix: &mut [Vec<f64>], rhs: &mut [f64]) -> Option<Vec<f64>> {
    let size = rhs.len();
    for column in 0..size {
        let pivot = (column..size).max_by(|left, right| {
            matrix[*left][column]
                .abs()
                .total_cmp(&matrix[*right][column].abs())
        })?;
        if matrix[pivot][column].abs() < 1e-14 {
            return None;
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let divisor = matrix[column][column];
        for row in column + 1..size {
            let factor = matrix[row][column] / divisor;
            if factor == 0.0 {
                continue;
            }
            for other in column..size {
                matrix[row][other] -= factor * matrix[column][other];
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let mut sum = rhs[row];
        for column in row + 1..size {
            sum -= matrix[row][column] * solution[column];
        }
        solution[row] = sum / matrix[row][row];
    }
    if solution.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(solution)
}

#[cfg(test)]
mod tests {
    use super::{Constraint, ConstraintKind, NodeRef, solve, validate};
    use crate::document::model::Object;
    use crate::document::{Document, Shape};
    use crate::geometry::{Path, Transform2};

    fn path_document(points: &[(f64, f64)], closed: bool) -> (Document, String) {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let object = Object::new(layer_id, Shape::Path(Path::from_points(points, closed)));
        let id = object.id.clone();
        document.add_object(object);
        (document, id)
    }

    fn nodes(document: &Document, id: &str) -> Vec<crate::geometry::Point2> {
        document
            .object(id)
            .unwrap()
            .world_path()
            .nodes()
            .into_iter()
            .map(|node| node.point)
            .collect()
    }

    fn with_constraint(document: &mut Document, kind: ConstraintKind) -> String {
        let constraint = Constraint::new(kind);
        let id = constraint.id.clone();
        document.constraints.push(constraint);
        id
    }

    #[test]
    fn horizontal_and_vertical_align_points() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 5.0), (20.0, 10.0)], false);
        let a = NodeRef::new(id.clone(), 0);
        let b = NodeRef::new(id.clone(), 1);
        with_constraint(
            &mut document,
            ConstraintKind::Horizontal {
                a: a.clone(),
                b: b.clone(),
            },
        );
        let (solved, report) = solve(&document, None).unwrap();
        let points = nodes(&solved, &id);
        assert!((points[0].y - points[1].y).abs() < 1e-6, "{points:?}");
        assert!(report.moved_nodes >= 1);
        assert!(report.residual <= 1e-6);

        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 5.0)], false);
        with_constraint(
            &mut document,
            ConstraintKind::Vertical {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let points = nodes(&solved, &id);
        assert!((points[0].x - points[1].x).abs() < 1e-6, "{points:?}");
    }

    #[test]
    fn a_driving_distance_sets_the_length() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        with_constraint(
            &mut document,
            ConstraintKind::Distance {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
                value_mm: 25.0,
            },
        );
        let (mut solved, report) = solve(&document, None).unwrap();
        assert_eq!(report.constraints, 1);
        let points = nodes(&solved, &id);
        let distance = points[0].distance(points[1]);
        assert!((distance - 25.0).abs() < 1e-5, "{distance}");

        // Changing the driving value re-solves from the current geometry.
        solved.constraints[0].kind.set_value(40.0);
        let (again, _) = solve(&solved, None).unwrap();
        let points = nodes(&again, &id);
        assert!((points[0].distance(points[1]) - 40.0).abs() < 1e-5);
    }

    #[test]
    fn coincident_joins_two_objects() {
        let (mut document, first) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        let second = {
            let layer_id = document.layers[0].id.clone();
            let object = Object::new(
                layer_id,
                Shape::Path(Path::from_points(&[(20.0, 5.0), (30.0, 5.0)], false)),
            );
            let id = object.id.clone();
            document.add_object(object);
            id
        };
        with_constraint(
            &mut document,
            ConstraintKind::Coincident {
                a: NodeRef::new(first.clone(), 1),
                b: NodeRef::new(second.clone(), 0),
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let end = nodes(&solved, &first)[1];
        let start = nodes(&solved, &second)[0];
        assert!(end.distance(start) < 1e-6, "{end:?} {start:?}");
    }

    #[test]
    fn parallel_and_perpendicular_turn_segments() {
        let (mut document, first) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        let second = {
            let layer_id = document.layers[0].id.clone();
            let object = Object::new(
                layer_id,
                Shape::Path(Path::from_points(&[(0.0, 5.0), (10.0, 8.0)], false)),
            );
            let id = object.id.clone();
            document.add_object(object);
            id
        };
        with_constraint(
            &mut document,
            ConstraintKind::Parallel {
                a: NodeRef::new(first.clone(), 0),
                b: NodeRef::new(second.clone(), 0),
            },
        );
        let (mut solved, report) = solve(&document, None).unwrap();
        assert_eq!(report.constraints, 1);
        let a = nodes(&solved, &first);
        let b = nodes(&solved, &second);
        let u = [a[1].x - a[0].x, a[1].y - a[0].y];
        let v = [b[1].x - b[0].x, b[1].y - b[0].y];
        let cross = u[0] * v[1] - u[1] * v[0];
        assert!(cross.abs() < 1e-5, "not parallel: {cross} {a:?} {b:?}");

        solved.constraints[0].kind = ConstraintKind::Perpendicular {
            a: NodeRef::new(first.clone(), 0),
            b: NodeRef::new(second.clone(), 0),
        };
        let (solved, _) = solve(&solved, None).unwrap();
        let a = nodes(&solved, &first);
        let b = nodes(&solved, &second);
        let u = [a[1].x - a[0].x, a[1].y - a[0].y];
        let v = [b[1].x - b[0].x, b[1].y - b[0].y];
        let dot = u[0] * v[0] + u[1] * v[1];
        assert!(dot.abs() < 1e-3, "not perpendicular: {dot} {a:?} {b:?}");
    }

    #[test]
    fn an_angle_turns_the_arms() {
        let (mut document, id) = path_document(&[(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)], false);
        with_constraint(
            &mut document,
            ConstraintKind::Angle {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
                c: NodeRef::new(id.clone(), 2),
                degrees: 45.0,
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let points = nodes(&solved, &id);
        let u = [points[0].x - points[1].x, points[0].y - points[1].y];
        let v = [points[2].x - points[1].x, points[2].y - points[1].y];
        let angle = f64::atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1]).to_degrees();
        assert!((angle - 45.0).abs() < 1e-4, "{angle} from {points:?}");
    }

    #[test]
    fn a_tangent_joins_endpoints_and_directions() {
        let (mut document, first) = path_document(&[(0.0, 0.0), (5.0, 0.0)], false);
        let second = {
            let layer_id = document.layers[0].id.clone();
            let object = Object::new(
                layer_id,
                Shape::Path(Path::from_points(&[(6.0, 1.0), (10.0, 4.0)], false)),
            );
            let id = object.id.clone();
            document.add_object(object);
            id
        };
        with_constraint(
            &mut document,
            ConstraintKind::Tangent {
                a: NodeRef::new(first.clone(), 0),
                b: NodeRef::new(second.clone(), 0),
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let a = nodes(&solved, &first);
        let b = nodes(&solved, &second);
        assert!(a[1].distance(b[0]) < 1e-6, "{a:?} {b:?}");
        let u = [a[1].x - a[0].x, a[1].y - a[0].y];
        let v = [b[1].x - b[0].x, b[1].y - b[0].y];
        let cross = u[0] * v[1] - u[1] * v[0];
        assert!(cross.abs() < 1e-5, "not collinear: {cross}");
    }

    #[test]
    fn conflicting_dimensions_leave_the_document_alone() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        let a = NodeRef::new(id.clone(), 0);
        let b = NodeRef::new(id.clone(), 1);
        with_constraint(
            &mut document,
            ConstraintKind::Distance {
                a: a.clone(),
                b: b.clone(),
                value_mm: 10.0,
            },
        );
        with_constraint(
            &mut document,
            ConstraintKind::Distance {
                a,
                b,
                value_mm: 20.0,
            },
        );
        let before = document.clone();
        let error = solve(&document, None).unwrap_err();
        assert!(error.to_string().contains("converge"), "{error}");
        assert_eq!(document, before);
    }

    #[test]
    fn disabled_constraints_are_ignored() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 5.0)], false);
        with_constraint(
            &mut document,
            ConstraintKind::Horizontal {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
            },
        );
        document.constraints[0].enabled = false;
        let (solved, report) = solve(&document, None).unwrap();
        assert_eq!(report.constraints, 0);
        assert_eq!(solved, document);
    }

    #[test]
    fn a_subset_of_constraints_can_be_solved() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 5.0), (20.0, 1.0)], false);
        let first = with_constraint(
            &mut document,
            ConstraintKind::Horizontal {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
            },
        );
        with_constraint(
            &mut document,
            ConstraintKind::Vertical {
                a: NodeRef::new(id.clone(), 1),
                b: NodeRef::new(id.clone(), 2),
            },
        );
        let (solved, report) = solve(&document, Some(&[first])).unwrap();
        assert_eq!(report.constraints, 1);
        let points = nodes(&solved, &id);
        assert!((points[0].y - points[1].y).abs() < 1e-6);
        // The vertical constraint was not selected, so x1 and x2 stay apart.
        assert!((points[1].x - points[2].x).abs() > 1.0);
    }

    #[test]
    fn a_closed_path_wraps_the_last_segment() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0), (5.0, 10.0)], true);
        with_constraint(
            &mut document,
            ConstraintKind::Horizontal {
                a: NodeRef::new(id.clone(), 2),
                b: NodeRef::new(id.clone(), 0),
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let points = nodes(&solved, &id);
        // Solving on a closed path keeps the closing segment.
        assert_eq!(points.len(), 3);
        assert!(solved.object(&id).is_some());
    }

    #[test]
    fn transforms_are_baked_before_solving() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        document.object_mut(&id).unwrap().transform = Transform2::translate(100.0, 50.0);
        with_constraint(
            &mut document,
            ConstraintKind::Distance {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 1),
                value_mm: 30.0,
            },
        );
        let (solved, _) = solve(&document, None).unwrap();
        let object = solved.object(&id).unwrap();
        assert_eq!(object.transform, Transform2::identity());
        let points = nodes(&solved, &id);
        assert!((points[0].distance(points[1]) - 30.0).abs() < 1e-5);
        assert!(
            points[0].x > 90.0,
            "the baked position was lost: {points:?}"
        );
    }

    #[test]
    fn bad_references_are_rejected() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        let missing = Constraint::new(ConstraintKind::Coincident {
            a: NodeRef::new("missing", 0),
            b: NodeRef::new(id.clone(), 1),
        });
        document.constraints.push(missing);
        assert!(validate(&document.constraints, &document).is_err());
        assert!(solve(&document, None).is_err());

        document.constraints[0].kind = ConstraintKind::Coincident {
            a: NodeRef::new(id.clone(), 9),
            b: NodeRef::new(id.clone(), 1),
        };
        assert!(validate(&document.constraints, &document).is_err());

        document.constraints[0].kind = ConstraintKind::Parallel {
            a: NodeRef::new(id.clone(), 0),
            b: NodeRef::new(id.clone(), 1),
        };
        // Node 1 has no segment leaving it on an open two-node path.
        assert!(validate(&document.constraints, &document).is_err());

        document.constraints[0].kind = ConstraintKind::Distance {
            a: NodeRef::new(id.clone(), 0),
            b: NodeRef::new(id.clone(), 1),
            value_mm: -3.0,
        };
        assert!(validate(&document.constraints, &document).is_err());
    }

    #[test]
    fn curved_segments_cannot_take_part_in_parallel_constraints() {
        let (mut document, id) = path_document(&[(0.0, 0.0), (10.0, 0.0)], false);
        // Give the path a curved segment.
        let path = crate::geometry::Path::from_nodes(
            &[
                crate::geometry::path::Node::new(crate::geometry::Point2::new(0.0, 0.0)),
                crate::geometry::path::Node {
                    point: crate::geometry::Point2::new(10.0, 0.0),
                    handle_in: Some(crate::geometry::Point2::new(8.0, 4.0)),
                    handle_out: Some(crate::geometry::Point2::new(12.0, 4.0)),
                },
            ],
            false,
        );
        document.object_mut(&id).unwrap().shape = Shape::Path(path);
        document
            .constraints
            .push(Constraint::new(ConstraintKind::Parallel {
                a: NodeRef::new(id.clone(), 0),
                b: NodeRef::new(id.clone(), 0),
            }));
        assert!(validate(&document.constraints, &document).is_err());
    }
}
