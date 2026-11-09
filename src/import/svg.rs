#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names
)]

use std::collections::HashMap;
use std::path::Path as FsPath;

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use crate::color::normalize_hex_color;
use crate::document::Document;
use crate::document::model::Object;
use crate::document::shape::{Shape, Style};
use crate::errors::{BeamError, Result};
use crate::geometry::{CubicBezier, Path, Point2, Transform2, path::Segment};
use crate::objects::{ImageObject, TextObject};

/// CSS pixels per inch used when the file gives no unit.
const DEFAULT_DPI: f64 = 96.0;

/// Largest external SVG accepted through a `<use>` reference.
const MAX_SVG_BYTES: u64 = 16 * 1_048_576;

/// Ways an SVG generator states its own pixel density.
///
/// Some editors write unitless `width`/`height` values at 72 dpi; SVG itself
/// assumes 96. The source metadata is enough to pick the right one without
/// asking the user.
fn detect_dpi(src: &str) -> f64 {
    let head: String = src.chars().take(4096).collect();
    let head = head.to_ascii_lowercase();
    if head.contains("adobe illustrator") {
        72.0
    } else {
        DEFAULT_DPI
    }
}

/// Deepest element nesting accepted in an SVG document.
///
/// The importer walks groups recursively and the XML tree is dropped
/// recursively too, so a pathological document must be refused before the tree
/// is built. Real drawings nest a few dozen levels at most.
pub const MAX_SVG_NESTING: usize = 256;

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub document: Document,
    pub warnings: Vec<String>,
}

/// Import an SVG document. Unsupported elements are skipped and reported by
/// [`import_svg_report`].
pub fn import_svg(src: &str) -> Result<Document> {
    Ok(import_svg_report(src)?.document)
}

pub fn import_svg_report(src: &str) -> Result<ImportReport> {
    import_svg_report_cancellable(src, &|| false)
}

/// Import an SVG, checking `cancel` between XML events and top-level elements.
pub fn import_svg_report_cancellable(src: &str, cancel: &dyn Fn() -> bool) -> Result<ImportReport> {
    import_svg_report_with_base(src, None, cancel)
}

/// Import an SVG from a file, allowing `<use>` to reference sibling `.svg` files.
pub fn import_svg_file_report_cancellable(
    path: &FsPath,
    cancel: &dyn Fn() -> bool,
) -> Result<ImportReport> {
    import_svg_file_report(path, cancel)
}

fn import_svg_file_report(path: &FsPath, cancel: &dyn Fn() -> bool) -> Result<ImportReport> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_SVG_BYTES {
        return Err(BeamError::InvalidInput(format!(
            "the SVG is larger than the {} MB limit",
            MAX_SVG_BYTES / 1_048_576
        )));
    }
    let source = std::fs::read_to_string(path)?;
    let base = path.parent();
    import_svg_report_with_base(&source, base, cancel)
}

fn import_svg_report_with_base(
    src: &str,
    base_dir: Option<&FsPath>,
    cancel: &dyn Fn() -> bool,
) -> Result<ImportReport> {
    let mut root = parse_tree_cancellable(src, cancel)?;
    if root.name != "svg" {
        return Err(BeamError::InvalidInput(
            "SVG root element is not <svg>".to_string(),
        ));
    }

    let dpi = detect_dpi(src);
    let mut warnings = Vec::new();
    resolve_gradient_paints(&mut root, &mut warnings);
    let mut doc = Document::default();
    doc.layers[0].name = "Imported".to_string();

    let width = root
        .attr("width")
        .and_then(parse_length)
        .and_then(|length| length.to_mm(dpi));
    let height = root
        .attr("height")
        .and_then(parse_length)
        .and_then(|length| length.to_mm(dpi));
    let view_box = root.attr("viewBox").and_then(parse_view_box);

    if let Some((_, _, vb_width, vb_height)) = view_box {
        doc.width_mm = width.unwrap_or(vb_width);
        doc.height_mm = height.unwrap_or(vb_height);
    } else {
        doc.width_mm = width.unwrap_or(300.0);
        doc.height_mm = height.unwrap_or(200.0);
    }
    if !doc.width_mm.is_finite() || doc.width_mm <= 0.0 {
        doc.width_mm = 300.0;
    }
    if !doc.height_mm.is_finite() || doc.height_mm <= 0.0 {
        doc.height_mm = 200.0;
    }

    let root_transform = root_transform(&root, view_box, doc.width_mm, doc.height_mm, dpi);
    let root_style = SvgStyle::from_node(&root);
    let layer_id = doc.layers[0].id.clone();
    let mut ids = HashMap::new();
    collect_ids(&root, &mut ids);

    for child in &root.children {
        if cancel() {
            return Err(BeamError::InvalidInput("cancelled".to_string()));
        }
        process_node(
            child,
            root_transform,
            &root_style,
            &mut doc,
            &layer_id,
            &mut warnings,
            &ids,
            0,
            dpi,
            base_dir,
        );
    }

    doc.validate()
        .map_err(|error| BeamError::InvalidInput(format!("imported SVG is invalid: {error}")))?;
    Ok(ImportReport {
        document: doc,
        warnings,
    })
}

#[derive(Debug, Default, Clone)]
struct Node {
    name: String,
    attrs: HashMap<String, String>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.attr(name)
            .and_then(|value| value.trim().parse::<f64>().ok())
    }
}

fn parse_tree(src: &str) -> Result<Node> {
    parse_tree_cancellable(src, &|| false)
}

fn parse_tree_cancellable(src: &str, cancel: &dyn Fn() -> bool) -> Result<Node> {
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    loop {
        if cancel() {
            return Err(BeamError::InvalidInput("cancelled".to_string()));
        }
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if stack.len() >= MAX_SVG_NESTING {
                    return Err(BeamError::InvalidInput(format!(
                        "SVG nesting is deeper than {MAX_SVG_NESTING} elements"
                    )));
                }
                let node = make_node(&event);
                stack.push(node);
            }
            Ok(Event::Empty(event)) => {
                let node = make_node(&event);
                attach(&mut stack, &mut root, node);
            }
            Ok(Event::End(_)) => {
                if let Some(node) = stack.pop() {
                    attach(&mut stack, &mut root, node);
                }
            }
            Ok(Event::Text(event)) => {
                if let Some(current) = stack.last_mut() {
                    current
                        .text
                        .push_str(&event.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Ok(Event::CData(event)) => {
                if let Some(current) = stack.last_mut() {
                    current
                        .text
                        .push_str(&event.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(BeamError::InvalidInput(format!("invalid SVG XML: {error}")));
            }
        }
    }

    root.ok_or_else(|| BeamError::InvalidInput("empty SVG document".to_string()))
}

fn make_node(event: &quick_xml::events::BytesStart<'_>) -> Node {
    let name = event.local_name().into_inner().to_string();
    let mut attrs = HashMap::new();
    for attribute in event.attributes().flatten() {
        let key = attribute.key.local_name().into_inner().to_string();
        attrs.insert(key, attribute.value.trim().to_string());
    }
    Node {
        name,
        attrs,
        children: Vec::new(),
        text: String::new(),
    }
}

fn attach(stack: &mut [Node], root: &mut Option<Node>, node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.is_none() {
        *root = Some(node);
    }
}

fn parse_view_box(value: &str) -> Option<(f64, f64, f64, f64)> {
    let values: Vec<f64> = value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();
    if values.len() == 4 && values[2] > 0.0 && values[3] > 0.0 {
        Some((values[0], values[1], values[2], values[3]))
    } else {
        None
    }
}

fn root_transform(
    root: &Node,
    view_box: Option<(f64, f64, f64, f64)>,
    width_mm: f64,
    height_mm: f64,
    dpi: f64,
) -> Transform2 {
    if let Some((vb_x, vb_y, vb_width, vb_height)) = view_box {
        let sx = width_mm / vb_width;
        let sy = height_mm / vb_height;
        let preserve = root
            .attr("preserveAspectRatio")
            .unwrap_or("xMidYMid meet")
            .trim();
        if preserve.eq_ignore_ascii_case("none") {
            return Transform2 {
                a: sx,
                b: 0.0,
                c: 0.0,
                d: -sy,
                tx: -vb_x * sx,
                ty: height_mm + vb_y * sy,
            };
        }
        let scale = sx.min(sy);
        let offset_x = (width_mm - vb_width * scale) / 2.0;
        let offset_y = (height_mm - vb_height * scale) / 2.0;
        return Transform2 {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: -scale,
            tx: offset_x - vb_x * scale,
            ty: height_mm - offset_y + vb_y * scale,
        };
    }
    let px_to_mm = 25.4 / dpi;
    Transform2 {
        a: px_to_mm,
        b: 0.0,
        c: 0.0,
        d: -px_to_mm,
        tx: 0.0,
        ty: height_mm,
    }
}

/// Every element with an `id`, used to expand `<use>` references.
fn collect_ids<'a>(node: &'a Node, ids: &mut HashMap<String, &'a Node>) {
    if let Some(id) = node.attr("id") {
        ids.entry(id.to_string()).or_insert(node);
    }
    for child in &node.children {
        collect_ids(child, ids);
    }
}

/// Expand `<use href="sibling.svg#id">` when the importer knows its folder.
#[allow(clippy::too_many_arguments)]
fn expand_external_use(
    node: &Node,
    file: &str,
    id: &str,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
    depth: usize,
    dpi: f64,
    base_dir: Option<&FsPath>,
) {
    let Some(base) = base_dir else {
        warnings.push(format!(
            "<use> to the external reference '{file}#{id}' needs a file path"
        ));
        return;
    };
    let candidate = base.join(file);
    if candidate
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("svg"))
    {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: only .svg files are supported"
        ));
        return;
    }
    let canonical = match std::fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(error) => {
            warnings.push(format!("<use> to '{file}#{id}' was skipped: {error}"));
            return;
        }
    };
    let base_canonical = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    if !canonical.starts_with(&base_canonical) {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: outside the import folder"
        ));
        return;
    }
    if depth >= 8 {
        warnings.push("<use> nesting is too deep".to_string());
        return;
    }
    if std::fs::metadata(&canonical).map_or(0, |metadata| metadata.len()) > MAX_SVG_BYTES {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: the file is too large"
        ));
        return;
    }
    let Ok(source) = std::fs::read_to_string(&canonical) else {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: cannot read the file"
        ));
        return;
    };
    let Ok(external) = parse_tree(&source) else {
        warnings.push(format!("<use> to '{file}#{id}' was skipped: invalid SVG"));
        return;
    };
    let mut external_ids = HashMap::new();
    collect_ids(&external, &mut external_ids);
    let Some(target) = external_ids.get(id).copied() else {
        warnings.push(format!("<use> to '{file}#{id}' was skipped: missing #{id}"));
        return;
    };
    let offset = Transform2::translate(
        node.number("x").unwrap_or(0.0),
        node.number("y").unwrap_or(0.0),
    );
    let transform = transform
        .compose(&offset)
        .compose(&use_view_box_transform(target, node));
    let next_base = canonical.parent();
    match target.name.as_str() {
        "symbol" | "g" | "svg" => {
            for child in &target.children {
                process_node(
                    child,
                    transform,
                    style,
                    doc,
                    layer_id,
                    warnings,
                    &external_ids,
                    depth + 1,
                    dpi,
                    next_base,
                );
            }
        }
        _ => process_node(
            target,
            transform,
            style,
            doc,
            layer_id,
            warnings,
            &external_ids,
            depth + 1,
            dpi,
            next_base,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn process_node(
    node: &Node,
    parent_transform: Transform2,
    parent_style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
    ids: &HashMap<String, &Node>,
    depth: usize,
    dpi: f64,
    base_dir: Option<&FsPath>,
) {
    let local = node
        .attr("transform")
        .map(parse_transform)
        .unwrap_or_default();
    let transform = parent_transform.compose(&local);
    let style = parent_style.inherit(node);

    match node.name.as_str() {
        "g" | "a" | "svg" => {
            for child in &node.children {
                process_node(
                    child, transform, &style, doc, layer_id, warnings, ids, depth, dpi, base_dir,
                );
            }
        }
        "defs" | "clipPath" | "mask" | "symbol" | "pattern" | "marker" | "metadata" | "title"
        | "desc" | "style" => {}
        "use" => {
            let href = node
                .attr("href")
                .or_else(|| node.attr("xlink:href"))
                .map(str::trim)
                .unwrap_or_default();
            if href.is_empty() {
                warnings.push("<use> without href was skipped".to_string());
            } else if let Some(id) = href.strip_prefix('#') {
                match ids.get(id) {
                    None => warnings.push(format!("<use> references missing #{id}")),
                    Some(_) if depth >= 8 => {
                        warnings.push("<use> nesting is too deep".to_string());
                    }
                    Some(target) => {
                        let offset = Transform2::translate(
                            node.number("x").unwrap_or(0.0),
                            node.number("y").unwrap_or(0.0),
                        );
                        let transform = transform
                            .compose(&offset)
                            .compose(&use_view_box_transform(target, node));
                        match target.name.as_str() {
                            "symbol" | "g" | "svg" => {
                                for child in &target.children {
                                    process_node(
                                        child,
                                        transform,
                                        &style,
                                        doc,
                                        layer_id,
                                        warnings,
                                        ids,
                                        depth + 1,
                                        dpi,
                                        base_dir,
                                    );
                                }
                            }
                            _ => process_node(
                                target,
                                transform,
                                &style,
                                doc,
                                layer_id,
                                warnings,
                                ids,
                                depth + 1,
                                dpi,
                                base_dir,
                            ),
                        }
                    }
                }
            } else if let Some((file, id)) = href.split_once('#') {
                expand_external_use(
                    node, file, id, transform, &style, doc, layer_id, warnings, depth, dpi,
                    base_dir,
                );
            } else {
                warnings.push(format!(
                    "<use> to the external reference '{href}' has no #id"
                ));
            }
        }
        "rect" => import_rect(node, transform, &style, doc, layer_id, warnings),
        "circle" => import_circle(node, transform, &style, doc, layer_id, warnings),
        "ellipse" => import_ellipse(node, transform, &style, doc, layer_id, warnings),
        "line" => import_line(node, transform, &style, doc, layer_id),
        "polyline" => import_poly(node, transform, &style, doc, layer_id, false, warnings),
        "polygon" => import_poly(node, transform, &style, doc, layer_id, true, warnings),
        "path" => import_path(node, transform, &style, doc, layer_id, warnings),
        "text" => import_text(node, transform, &style, doc, layer_id),
        "image" => import_image(node, transform, &style, doc, layer_id, dpi),
        other => warnings.push(format!("unsupported SVG element: <{other}>")),
    }
}

/// Map a referenced `symbol`/`svg` viewBox into the size given on `<use>`.
///
/// Without an explicit width and height the referenced content keeps its own
/// coordinates. `preserveAspectRatio="none"` stretches; otherwise the default
/// `meet` rule scales uniformly and centres the result.
fn use_view_box_transform(target: &Node, use_node: &Node) -> Transform2 {
    let Some((view_x, view_y, view_width, view_height)) =
        target.attr("viewBox").and_then(parse_view_box)
    else {
        return Transform2::identity();
    };
    let (Some(width), Some(height)) = (use_node.number("width"), use_node.number("height")) else {
        return Transform2::identity();
    };
    if width <= 0.0 || height <= 0.0 {
        return Transform2::identity();
    }
    let ratio_x = width / view_width;
    let ratio_y = height / view_height;
    let aspect = target
        .attr("preserveAspectRatio")
        .or_else(|| use_node.attr("preserveAspectRatio"))
        .unwrap_or("xMidYMid meet");
    let (scale_x, scale_y, extra_x, extra_y) = if aspect.trim().starts_with("none") {
        (ratio_x, ratio_y, 0.0, 0.0)
    } else {
        let scale = ratio_x.min(ratio_y);
        (
            scale,
            scale,
            width - view_width * scale,
            height - view_height * scale,
        )
    };
    let align = aspect.split_whitespace().next().unwrap_or("xMidYMid");
    let x_extra = if align.contains("xMin") {
        0.0
    } else if align.contains("xMax") {
        extra_x
    } else {
        extra_x / 2.0
    };
    let y_extra = if align.contains("YMin") {
        0.0
    } else if align.contains("YMax") {
        extra_y
    } else {
        extra_y / 2.0
    };
    Transform2::translate(x_extra, y_extra)
        .compose(&Transform2::scale(scale_x, scale_y))
        .compose(&Transform2::translate(-view_x, -view_y))
}

/// Whether two closed contours overlap with neither containing the other.
fn contours_partially_overlap(first: &[(f64, f64)], second: &[(f64, f64)]) -> bool {
    use crate::geometry::boolean::{BooleanOp, boolean_contours, polygon_area};

    let overlap = boolean_contours(BooleanOp::Intersection, first, second);
    if overlap.is_empty() {
        return false;
    }
    let minimum = polygon_area(first).abs().min(polygon_area(second).abs());
    let covered: f64 = overlap
        .iter()
        .map(|contour| polygon_area(contour).abs())
        .sum();
    minimum > 0.0 && covered < minimum * 0.999
}

fn import_rect(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
) {
    let x = node.number("x").unwrap_or(0.0);
    let y = node.number("y").unwrap_or(0.0);
    let width = node.number("width").unwrap_or(0.0);
    let height = node.number("height").unwrap_or(0.0);
    if width <= 0.0 || height <= 0.0 {
        warnings.push("skipped a rectangle with non-positive size".to_string());
        return;
    }
    let corner_radius = node
        .number("rx")
        .or_else(|| node.number("ry"))
        .unwrap_or(0.0);
    let shape = Shape::Rect {
        width,
        height,
        corner_radius,
    };
    let object = Object::new(layer_id, shape)
        .with_transform(transform.compose(&Transform2::translate(x, y)))
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn import_circle(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
) {
    let radius = node.number("r").unwrap_or(0.0);
    if radius <= 0.0 {
        warnings.push("skipped a circle with non-positive radius".to_string());
        return;
    }
    let cx = node.number("cx").unwrap_or(0.0);
    let cy = node.number("cy").unwrap_or(0.0);
    let object = Object::new(
        layer_id,
        Shape::Ellipse {
            rx: radius,
            ry: radius,
        },
    )
    .with_transform(transform.compose(&Transform2::translate(cx, cy)))
    .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn import_ellipse(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
) {
    let rx = node.number("rx").unwrap_or(0.0);
    let ry = node.number("ry").unwrap_or(0.0);
    if rx <= 0.0 || ry <= 0.0 {
        warnings.push("skipped an ellipse with non-positive radii".to_string());
        return;
    }
    let cx = node.number("cx").unwrap_or(0.0);
    let cy = node.number("cy").unwrap_or(0.0);
    let object = Object::new(layer_id, Shape::Ellipse { rx, ry })
        .with_transform(transform.compose(&Transform2::translate(cx, cy)))
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn import_line(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
) {
    let x1 = node.number("x1").unwrap_or(0.0);
    let y1 = node.number("y1").unwrap_or(0.0);
    let x2 = node.number("x2").unwrap_or(0.0);
    let y2 = node.number("y2").unwrap_or(0.0);
    let path = Path {
        segments: vec![Segment::Line {
            from: Point2::new(x1, y1),
            to: Point2::new(x2, y2),
        }],
        closed: false,
    };
    let object = Object::new(layer_id, Shape::Path(path))
        .with_transform(transform)
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn import_poly(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    closed: bool,
    warnings: &mut Vec<String>,
) {
    let Some(points) = node.attr("points") else {
        return;
    };
    let values: Vec<f64> = points
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<f64>().ok())
        .collect();
    if values.len() < 4 || values.len() % 2 != 0 {
        warnings.push("skipped a polyline/polygon with invalid points".to_string());
        return;
    }
    let pairs: Vec<(f64, f64)> = values
        .chunks_exact(2)
        .map(|chunk| (chunk[0], chunk[1]))
        .collect();
    let path = Path::from_points(&pairs, closed);
    let object = Object::new(layer_id, Shape::Path(path))
        .with_transform(transform)
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn import_path(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
) {
    let Some(data) = node.attr("d") else {
        return;
    };
    let bez = match kurbo::BezPath::from_svg(data) {
        Ok(bez) => bez,
        Err(error) => {
            warnings.push(format!("skipped an invalid path: {error}"));
            return;
        }
    };
    import_paths(
        bezpath_to_paths(&bez),
        transform,
        style,
        doc,
        layer_id,
        warnings,
    );
}

/// Add the subpaths of one SVG `<path>`.
///
/// A compound path keeps its holes: contours are nested by containment, and an
/// odd depth marks a hole of the nearest containing contour. G-code fills and
/// PNG fills then leave that area empty instead of filling it as a separate
/// shape.
/// Union every pair of overlapping contours until no pair can merge.
fn weld_contours(mut parts: Vec<Vec<(f64, f64)>>) -> Vec<Vec<(f64, f64)>> {
    use crate::geometry::boolean::{BooleanOp, boolean_contours, polygon_area};

    loop {
        let mut merged = false;
        'search: for first in 0..parts.len() {
            for second in (first + 1)..parts.len() {
                let union = boolean_contours(BooleanOp::Union, &parts[first], &parts[second]);
                if union.is_empty() {
                    continue;
                }
                let before = polygon_area(&parts[first]).abs() + polygon_area(&parts[second]).abs();
                let after: f64 = union
                    .iter()
                    .map(|contour| polygon_area(contour).abs())
                    .sum();
                if after < before - 1e-6 {
                    parts.splice(first..=second, union);
                    merged = true;
                    break 'search;
                }
            }
        }
        if !merged {
            return parts;
        }
    }
}

fn import_paths(
    paths: Vec<Path>,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    warnings: &mut Vec<String>,
) {
    if paths.len() < 2 {
        for path in paths {
            add_path_object(path, transform, style, doc, layer_id);
        }
        return;
    }
    if paths.len() == 2
        && paths.iter().all(|path| path.closed)
        && style
            .fill
            .as_ref()
            .is_some_and(std::option::Option::is_some)
    {
        let a: Vec<(f64, f64)> = paths[0]
            .flatten(0.05)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        let b: Vec<(f64, f64)> = paths[1]
            .flatten(0.05)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        let overlap = crate::geometry::boolean::boolean_contours(
            crate::geometry::boolean::BooleanOp::Intersection,
            &a,
            &b,
        );
        let contained = {
            let minimum = crate::geometry::boolean::polygon_area(&a)
                .abs()
                .min(crate::geometry::boolean::polygon_area(&b).abs());
            let covered: f64 = overlap
                .iter()
                .map(|contour| crate::geometry::boolean::polygon_area(contour).abs())
                .sum();
            minimum > 0.0 && covered >= minimum * 0.999
        };
        if !overlap.is_empty() && !contained {
            let winding_differs = crate::geometry::boolean::polygon_area(&a).signum()
                != crate::geometry::boolean::polygon_area(&b).signum();
            let op = if style.even_odd() || winding_differs {
                crate::geometry::boolean::BooleanOp::Xor
            } else {
                crate::geometry::boolean::BooleanOp::Union
            };
            let contours = crate::geometry::boolean::boolean_contours(op, &a, &b);
            if !contours.is_empty() {
                warnings
                    .push("overlapping contours were resolved as straight segments".to_string());
                let resolved: Vec<Path> = contours
                    .iter()
                    .map(|contour| {
                        let points: Vec<Point2> =
                            contour.iter().map(|(x, y)| Point2::new(*x, *y)).collect();
                        crate::vectorize::to_path(&points)
                    })
                    .collect();
                let parents = crate::geometry::path::contour_parents(&resolved);
                let mut order: Vec<usize> = (0..resolved.len()).collect();
                order.sort_by_key(|&index| parents[index].is_some());
                let mut ids: Vec<Option<String>> = vec![None; resolved.len()];
                for index in order {
                    let mut object = Object::new(layer_id, Shape::Path(resolved[index].clone()))
                        .with_transform(transform)
                        .with_style(style.to_object_style(&transform));
                    if let Some(parent_id) = parents[index].and_then(|parent| ids[parent].clone()) {
                        object.hole_of = Some(parent_id);
                        object.name = "Hole".to_string();
                    }
                    ids[index] = Some(doc.add_object(object).id.clone());
                }
                return;
            }
        }
    }
    if paths.len() > 2
        && paths.iter().all(|path| path.closed)
        && style
            .fill
            .as_ref()
            .is_some_and(std::option::Option::is_some)
    {
        let polygons: Vec<Vec<(f64, f64)>> = paths
            .iter()
            .map(|path| {
                path.flatten(0.05)
                    .into_iter()
                    .map(|point| (point.x, point.y))
                    .collect()
            })
            .collect();
        let same_winding = polygons.iter().all(|polygon| {
            crate::geometry::boolean::polygon_area(polygon).signum()
                == crate::geometry::boolean::polygon_area(&polygons[0]).signum()
        });
        if same_winding && !style.even_odd() {
            let welded = weld_contours(polygons.clone());
            if welded.len() < polygons.len() {
                warnings
                    .push("overlapping contours were resolved as straight segments".to_string());
                let resolved: Vec<Path> = welded
                    .iter()
                    .map(|contour| {
                        let points: Vec<Point2> =
                            contour.iter().map(|(x, y)| Point2::new(*x, *y)).collect();
                        crate::vectorize::to_path(&points)
                    })
                    .collect();
                let parents = crate::geometry::path::contour_parents(&resolved);
                let mut order: Vec<usize> = (0..resolved.len()).collect();
                order.sort_by_key(|&index| parents[index].is_some());
                let mut ids: Vec<Option<String>> = vec![None; resolved.len()];
                for index in order {
                    let mut object = Object::new(layer_id, Shape::Path(resolved[index].clone()))
                        .with_transform(transform)
                        .with_style(style.to_object_style(&transform));
                    if let Some(parent_id) = parents[index].and_then(|parent| ids[parent].clone()) {
                        object.hole_of = Some(parent_id);
                        object.name = "Hole".to_string();
                    }
                    ids[index] = Some(doc.add_object(object).id.clone());
                }
                return;
            }
        }
        let mut partial = false;
        'pairs: for first in 0..polygons.len() {
            for second in first + 1..polygons.len() {
                if contours_partially_overlap(&polygons[first], &polygons[second]) {
                    partial = true;
                    break 'pairs;
                }
            }
        }
        if partial {
            warnings.push(
                "three or more overlapping contours are imported as separate shapes".to_string(),
            );
        }
    }
    let parents = crate::geometry::path::contour_parents_with_rule(&paths, style.even_odd());
    let mut order: Vec<usize> = (0..paths.len()).collect();
    order.sort_by_key(|&index| parents[index].is_some());
    let mut ids: Vec<Option<String>> = vec![None; paths.len()];
    for index in order {
        let mut object = Object::new(layer_id, Shape::Path(paths[index].clone()))
            .with_transform(transform)
            .with_style(style.to_object_style(&transform));
        if let Some(parent_id) = parents[index].and_then(|parent| ids[parent].clone()) {
            object.hole_of = Some(parent_id);
            object.name = "Hole".to_string();
        }
        ids[index] = Some(doc.add_object(object).id.clone());
    }
}

fn add_path_object(
    path: Path,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
) {
    let object = Object::new(layer_id, Shape::Path(path))
        .with_transform(transform)
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

#[derive(Debug, Clone, Default)]
#[allow(clippy::option_option)]
struct SvgStyle {
    stroke: Option<Option<String>>,
    fill: Option<Option<String>>,
    stroke_width: Option<f64>,
    fill_rule: Option<String>,
}

impl SvgStyle {
    fn from_node(node: &Node) -> Self {
        let mut style = Self::default();
        style.apply_attributes(node);
        style
    }

    fn inherit(&self, node: &Node) -> Self {
        let mut style = self.clone();
        style.apply_attributes(node);
        style
    }

    fn apply_attributes(&mut self, node: &Node) {
        if let Some(value) = node.attr("fill") {
            self.fill = paint(value);
        }
        if let Some(value) = node.attr("stroke") {
            self.stroke = paint(value);
        }
        if let Some(value) = node.attr("stroke-width") {
            self.stroke_width = parse_length(value).map(Length::numeric);
        }
        if let Some(value) = node.attr("fill-rule") {
            self.fill_rule = Some(value.trim().to_ascii_lowercase());
        }
        if let Some(value) = node.attr("style") {
            for declaration in value.split(';') {
                let Some((key, value)) = declaration.split_once(':') else {
                    continue;
                };
                match key.trim() {
                    "fill" => self.fill = paint(value),
                    "stroke" => self.stroke = paint(value),
                    "stroke-width" => {
                        self.stroke_width = parse_length(value).map(Length::numeric);
                    }
                    "fill-rule" => {
                        self.fill_rule = Some(value.trim().to_ascii_lowercase());
                    }
                    _ => {}
                }
            }
        }
    }

    fn even_odd(&self) -> bool {
        self.fill_rule.as_deref() == Some("evenodd")
    }

    fn to_object_style(&self, transform: &Transform2) -> Style {
        let scale = transform_scale(transform);
        let stroke = self
            .stroke
            .clone()
            .unwrap_or_else(|| Some("#000000".to_string()));
        let fill = self.fill.clone().unwrap_or(None);
        Style {
            stroke,
            fill,
            stroke_width_mm: (self.stroke_width.unwrap_or(1.0) * scale).max(0.01),
        }
    }
}

fn import_text(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
) {
    let content = node.text.trim();
    if content.is_empty() {
        return;
    }
    let x = node.number("x").unwrap_or(0.0);
    let y = node.number("y").unwrap_or(0.0);
    let size = node
        .attr("font-size")
        .and_then(parse_length)
        .map_or(12.0, Length::numeric);
    let font = node
        .attr("font-family")
        .unwrap_or("sans")
        .trim()
        .to_string();
    let world = transform.apply(Point2::new(x, y));
    let scale = transform_scale(&transform);
    let align = match node.attr("text-anchor") {
        Some("middle") => crate::objects::TextAlign::Center,
        Some("end") => crate::objects::TextAlign::Right,
        _ => crate::objects::TextAlign::Left,
    };
    let letter_spacing = node
        .attr("letter-spacing")
        .and_then(parse_length)
        .map_or(0.0, |length| length.numeric() * scale);
    let text = TextObject {
        content: content.to_string(),
        font,
        size: (size * scale).max(0.1),
        x: world.x,
        y: world.y,
        letter_spacing,
        align,
        ..TextObject::default()
    };
    let mut object = Object::new(layer_id, Shape::Text(text));
    object.style = style.to_object_style(&transform);
    if object.style.fill.is_none() && object.style.stroke.is_none() {
        object.style.fill = Some("#000000".to_string());
    }
    doc.add_object(object);
}

fn import_image(
    node: &Node,
    transform: Transform2,
    style: &SvgStyle,
    doc: &mut Document,
    layer_id: &str,
    dpi: f64,
) {
    let Some(href) = node.attr("href").map(str::to_string) else {
        return;
    };
    if href.is_empty() {
        return;
    }
    let x = node.number("x").unwrap_or(0.0);
    let y = node.number("y").unwrap_or(0.0);
    let width = node.number("width").unwrap_or(1.0).max(0.1);
    let height = node.number("height").unwrap_or(1.0).max(0.1);
    let scale = transform_scale(&transform);
    let width_mm = width * scale;
    let height_mm = height * scale;
    let image = ImageObject::new(
        href,
        (width_mm * dpi / 25.4).round().max(1.0) as u32,
        (height_mm * dpi / 25.4).round().max(1.0) as u32,
        dpi,
    );
    let world = transform.apply(Point2::new(x, y));
    let object = Object::new(layer_id, Shape::Image(image))
        .with_transform(Transform2::translate(world.x, world.y))
        .with_style(style.to_object_style(&transform));
    doc.add_object(object);
}

fn bezpath_to_paths(bez: &kurbo::BezPath) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut current = Path::new();
    let mut last: Option<Point2> = None;

    for element in bez.elements() {
        match *element {
            kurbo::PathEl::MoveTo(point) => {
                if !current.segments.is_empty() {
                    paths.push(current);
                    current = Path::new();
                }
                last = Some(Point2::new(point.x, point.y));
            }
            kurbo::PathEl::LineTo(point) => {
                let to = Point2::new(point.x, point.y);
                if let Some(from) = last {
                    current.push(Segment::Line { from, to });
                }
                last = Some(to);
            }
            kurbo::PathEl::QuadTo(control, point) => {
                let to = Point2::new(point.x, point.y);
                if let Some(from) = last {
                    let control = Point2::new(control.x, control.y);
                    current.push(Segment::Cubic(CubicBezier {
                        p0: from,
                        p1: from.add(control.sub(from).scale(2.0 / 3.0)),
                        p2: to.add(control.sub(to).scale(2.0 / 3.0)),
                        p3: to,
                    }));
                }
                last = Some(to);
            }
            kurbo::PathEl::CurveTo(control_a, control_b, point) => {
                let to = Point2::new(point.x, point.y);
                if let Some(from) = last {
                    current.push(Segment::Cubic(CubicBezier {
                        p0: from,
                        p1: Point2::new(control_a.x, control_a.y),
                        p2: Point2::new(control_b.x, control_b.y),
                        p3: to,
                    }));
                }
                last = Some(to);
            }
            kurbo::PathEl::ClosePath => {
                current.closed = true;
                if !current.segments.is_empty() {
                    paths.push(current);
                    current = Path::new();
                }
                last = None;
            }
        }
    }
    if !current.segments.is_empty() {
        paths.push(current);
    }
    paths
}

fn transform_scale(transform: &Transform2) -> f64 {
    let sx = (transform.a * transform.a + transform.b * transform.b).sqrt();
    let sy = (transform.c * transform.c + transform.d * transform.d).sqrt();
    f64::midpoint(sx, sy).max(1e-9)
}

#[allow(clippy::option_option)]
/// Replace `url(#id)` paints with one representative colour.
///
/// Gradients use their first stop and patterns the first fill found inside
/// them. The approximation is reported once; a missing reference is reported by
/// id. Without this pass the shapes would silently lose their paint.
fn resolve_gradient_paints(root: &mut Node, warnings: &mut Vec<String>) {
    let mut by_id: HashMap<String, &Node> = HashMap::new();
    collect_ids(root, &mut by_id);
    let mut colors: HashMap<String, String> = HashMap::new();
    for (id, node) in &by_id {
        let color = match node.name.as_str() {
            "linearGradient" | "radialGradient" => gradient_color(node, &by_id, 0),
            "pattern" => pattern_color(node, 0),
            _ => None,
        };
        if let Some(color) = color {
            colors.insert(id.clone(), color);
        }
    }
    let mut approximate = false;
    let mut missing: Vec<String> = Vec::new();
    rewrite_paints(root, &colors, &mut approximate, &mut missing);
    if approximate {
        warnings.push(
            "gradients and patterns are approximated by one colour; the source is not reproduced"
                .to_string(),
        );
    }
    for id in missing {
        warnings.push(format!("paint reference #{id} was not found"));
    }
}

fn gradient_color(node: &Node, by_id: &HashMap<String, &Node>, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    if let Some(stop) = node.children.iter().find(|child| child.name == "stop") {
        if let Some(color) = stop_color(stop) {
            return Some(color);
        }
    }
    let href = node.attr("href").or_else(|| node.attr("xlink:href"))?;
    let id = href.trim().strip_prefix('#')?;
    gradient_color(by_id.get(id)?, by_id, depth + 1)
}

fn stop_color(stop: &Node) -> Option<String> {
    if let Some(value) = stop.attr("stop-color") {
        if let Some(Some(color)) = paint(value) {
            return Some(color);
        }
    }
    let style = stop.attr("style")?;
    for declaration in style.split(';') {
        let (key, value) = declaration.split_once(':')?;
        if key.trim() == "stop-color" {
            if let Some(Some(color)) = paint(value) {
                return Some(color);
            }
        }
    }
    None
}

fn pattern_color(node: &Node, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    for child in &node.children {
        if let Some(value) = child.attr("fill") {
            if let Some(Some(color)) = paint(value) {
                return Some(color);
            }
        }
        if let Some(color) = pattern_color(child, depth + 1) {
            return Some(color);
        }
    }
    None
}

fn rewrite_paints(
    node: &mut Node,
    colors: &HashMap<String, String>,
    approximate: &mut bool,
    missing: &mut Vec<String>,
) {
    for attribute in ["fill", "stroke"] {
        if let Some(value) = node.attrs.get(attribute).cloned() {
            if let Some((_, resolved)) = resolve_paint_url(&value, colors) {
                node.attrs.insert(attribute.to_string(), resolved);
                *approximate = true;
            } else if let Some(id) = paint_url_id(&value) {
                if !missing.contains(&id) {
                    missing.push(id);
                }
            }
        }
    }
    if let Some(style) = node.attrs.get("style").cloned() {
        let mut changed = false;
        let rewritten: Vec<String> = style
            .split(';')
            .map(|declaration| {
                let Some((key, value)) = declaration.split_once(':') else {
                    return declaration.to_string();
                };
                let key = key.trim();
                if key == "fill" || key == "stroke" {
                    if let Some((_, resolved)) = resolve_paint_url(value.trim(), colors) {
                        changed = true;
                        return format!("{key}:{resolved}");
                    }
                }
                declaration.to_string()
            })
            .collect();
        if changed {
            node.attrs.insert("style".to_string(), rewritten.join(";"));
        }
    }
    for child in &mut node.children {
        rewrite_paints(child, colors, approximate, missing);
    }
}

/// The id inside `url(#id)` or `url("#id")`.
fn paint_url_id(value: &str) -> Option<String> {
    let inner = value
        .trim()
        .strip_prefix("url(")?
        .strip_suffix(')')?
        .trim()
        .trim_matches(['"', '\'']);
    inner.strip_prefix('#').map(str::to_string)
}

fn resolve_paint_url(value: &str, colors: &HashMap<String, String>) -> Option<(String, String)> {
    let id = paint_url_id(value)?;
    let color = colors.get(&id)?.clone();
    Some((id, color))
}

#[allow(clippy::option_option)]
fn paint(value: &str) -> Option<Option<String>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return Some(None);
    }
    if let Some(color) = normalize_hex_color(value) {
        return Some(Some(color));
    }
    let named = match value.to_ascii_lowercase().as_str() {
        "black" | "currentcolor" => "#000000",
        "white" => "#ffffff",
        "red" => "#ff0000",
        "green" => "#008000",
        "blue" => "#0000ff",
        "yellow" => "#ffff00",
        "cyan" | "aqua" => "#00ffff",
        "magenta" | "fuchsia" => "#ff00ff",
        "gray" | "grey" => "#808080",
        "silver" => "#c0c0c0",
        "maroon" => "#800000",
        "olive" => "#808000",
        "lime" => "#00ff00",
        "navy" => "#000080",
        "teal" => "#008080",
        "purple" => "#800080",
        "orange" => "#ffa500",
        _ => return None,
    };
    Some(Some(named.to_string()))
}

#[derive(Debug, Clone, Copy)]
struct Length {
    value: f64,
    unit: LengthUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LengthUnit {
    None,
    Px,
    Mm,
    Cm,
    In,
    Pt,
    Pc,
    Percent,
}

impl Length {
    fn numeric(self) -> f64 {
        self.value
    }

    fn to_mm(self, dpi: f64) -> Option<f64> {
        match self.unit {
            LengthUnit::None | LengthUnit::Px => Some(self.value * 25.4 / dpi),
            LengthUnit::Mm => Some(self.value),
            LengthUnit::Cm => Some(self.value * 10.0),
            LengthUnit::In => Some(self.value * 25.4),
            LengthUnit::Pt => Some(self.value * 25.4 / 72.0),
            LengthUnit::Pc => Some(self.value * 25.4 / 6.0),
            LengthUnit::Percent => None,
        }
    }
}

fn parse_length(value: &str) -> Option<Length> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (number, unit) = if let Some(stripped) = value.strip_suffix("px") {
        (stripped, LengthUnit::Px)
    } else if let Some(stripped) = value.strip_suffix("mm") {
        (stripped, LengthUnit::Mm)
    } else if let Some(stripped) = value.strip_suffix("cm") {
        (stripped, LengthUnit::Cm)
    } else if let Some(stripped) = value.strip_suffix("in") {
        (stripped, LengthUnit::In)
    } else if let Some(stripped) = value.strip_suffix("pt") {
        (stripped, LengthUnit::Pt)
    } else if let Some(stripped) = value.strip_suffix("pc") {
        (stripped, LengthUnit::Pc)
    } else if let Some(stripped) = value.strip_suffix('%') {
        (stripped, LengthUnit::Percent)
    } else {
        (value, LengthUnit::None)
    };
    let value = number.trim().parse::<f64>().ok()?;
    Some(Length { value, unit })
}

fn parse_transform(value: &str) -> Transform2 {
    let mut result = Transform2::identity();
    let mut rest = value;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let args_text = &rest[open + 1..open + close];
        let args: Vec<f64> = args_text
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<f64>().ok())
            .collect();
        let transform = match name {
            "matrix" if args.len() == 6 => Transform2 {
                a: args[0],
                b: args[1],
                c: args[2],
                d: args[3],
                tx: args[4],
                ty: args[5],
            },
            "translate" if !args.is_empty() => {
                Transform2::translate(args[0], args.get(1).copied().unwrap_or(0.0))
            }
            "scale" if !args.is_empty() => {
                Transform2::scale(args[0], args.get(1).copied().unwrap_or(args[0]))
            }
            "rotate" if !args.is_empty() => {
                let angle = args[0].to_radians();
                if args.len() >= 3 {
                    Transform2::rotate_around(angle, args[1], args[2])
                } else {
                    Transform2::rotate(angle)
                }
            }
            "skewX" if !args.is_empty() => Transform2 {
                a: 1.0,
                b: 0.0,
                c: args[0].to_radians().tan(),
                d: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            "skewY" if !args.is_empty() => Transform2 {
                a: 1.0,
                b: args[0].to_radians().tan(),
                c: 0.0,
                d: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            _ => Transform2::identity(),
        };
        result = result.compose(&transform);
        rest = &rest[open + close + 1..];
    }
    result
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use super::*;

    const SVG_UNITS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="80mm" viewBox="0 0 100 80"><rect x="10" y="20" width="30" height="15"/></svg>"#;
    const SVG_GROUP: &str = r#"<svg width="100mm" height="100mm" viewBox="0 0 100 100"><g transform="translate(10,10) scale(2)"><rect x="0" y="0" width="10" height="10"/></g></svg>"#;
    const SVG_ARC: &str = r#"<svg width="100" height="100" viewBox="0 0 100 100"><path d="M10 10 L 50 10 A 20 20 0 0 1 50 50 Z"/></svg>"#;
    const SVG_SHAPES: &str = r#"<svg width="100" height="100" viewBox="0 0 100 100"><circle cx="50" cy="50" r="10"/><line x1="0" y1="0" x2="10" y2="10"/></svg>"#;
    const SVG_USE: &str = r##"<svg width="10" height="10"><use href="#x"/></svg>"##;
    const SVG_STYLE: &str = r##"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><rect width="1" height="1" stroke="none" fill="#ff0000" stroke-width="0.3"/></svg>"##;
    const SVG_TEXT: &str = r#"<svg><text x="1" y="2" font-size="4">hello</text></svg>"#;
    const SVG_COMPOUND: &str = r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><path d="M1 1 H9 V9 H1 Z M3 3 V7 H7 V3 Z"/></svg>"#;
    const SVG_COMPOUND_ISLAND: &str = r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><path d="M1 1 H9 V9 H1 Z M3 3 V7 H7 V3 Z M4 4 H6 V6 H4 Z"/></svg>"#;

    #[test]
    fn empty_svg() {
        let doc = import_svg("<svg></svg>").unwrap();
        assert!(doc.objects.is_empty());
    }

    #[test]
    fn imports_rect_with_units() {
        let doc = import_svg(SVG_UNITS).unwrap();
        assert_eq!(doc.objects.len(), 1);
        assert!((doc.width_mm - 100.0).abs() < 1e-6);
        assert!((doc.height_mm - 80.0).abs() < 1e-6);
        let (min, max) = doc.objects[0].bounds().unwrap();
        assert!((min.x - 10.0).abs() < 1e-6);
        assert!((max.x - 40.0).abs() < 1e-6);
        // SVG y is flipped: y=20..35 becomes 45..60 in an 80 mm document.
        assert!((min.y - 45.0).abs() < 1e-6);
        assert!((max.y - 60.0).abs() < 1e-6);
    }

    #[test]
    fn imports_group_transform() {
        let doc = import_svg(SVG_GROUP).unwrap();
        let (min, max) = doc.objects[0].bounds().unwrap();
        assert!((min.x - 10.0).abs() < 1e-6);
        assert!((max.x - 30.0).abs() < 1e-6);
    }

    #[test]
    fn refuses_pathological_nesting() {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm">{}{}<rect x="1" y="1" width="2" height="2"/>{}</svg>"#,
            "<g>".repeat(MAX_SVG_NESTING),
            "<g>",
            "</g>".repeat(MAX_SVG_NESTING + 1)
        );
        let error = import_svg(&svg).unwrap_err();
        assert!(error.to_string().contains("nesting"), "{error}");
    }

    #[test]
    fn accepts_reasonable_nesting() {
        let depth = 64;
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm">{}<rect x="1" y="1" width="2" height="2"/>{}</svg>"#,
            "<g>".repeat(depth),
            "</g>".repeat(depth)
        );
        let doc = import_svg(&svg).unwrap();
        assert_eq!(doc.objects.len(), 1);
    }

    #[test]
    fn a_compound_path_keeps_its_hole() {
        let doc = import_svg(SVG_COMPOUND).unwrap();
        assert_eq!(doc.objects.len(), 2);
        let hole = doc
            .objects
            .iter()
            .find(|object| object.hole_of.is_some())
            .expect("the inner contour is not marked as a hole");
        let outer = doc
            .objects
            .iter()
            .find(|object| object.id != hole.id)
            .expect("the outer contour is missing");
        assert_eq!(hole.hole_of.as_deref(), Some(outer.id.as_str()));
        let (min, max) = hole.bounds().unwrap();
        assert!((min.x - 3.0).abs() < 1e-6);
        assert!((min.y - 3.0).abs() < 1e-6);
        assert!((max.x - 7.0).abs() < 1e-6);
        assert!((max.y - 7.0).abs() < 1e-6);
    }

    #[test]
    fn three_same_winding_contours_are_united() {
        let report = import_svg_report(
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path fill="black" d="M1 1 H9 V9 H1 Z M5 5 H13 V13 H5 Z M8 8 H16 V16 H8 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 16.0).abs() < 1e-6, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("overlapping contours were resolved")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn three_overlapping_contours_are_reported() {
        let report = import_svg_report(
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path fill="black" d="M1 1 H9 V9 H1 Z M5 5 V13 H13 V5 Z M8 8 H16 V16 H8 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 3);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("three or more overlapping contours")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn opposing_overlaps_cancel_under_nonzero() {
        let report = import_svg_report(
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path fill="black" d="M1 1 H9 V9 H1 Z M5 5 V15 H15 V5 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(
            report.document.objects.len(),
            2,
            "the overlap was not split"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("overlapping contours were resolved")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn same_winding_overlaps_are_united() {
        let report = import_svg_report(
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path fill="black" d="M1 1 H9 V9 H1 Z M5 5 H15 V15 H5 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(
            report.document.objects.len(),
            1,
            "same-winding overlap was not united"
        );
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 15.0).abs() < 1e-6, "{max:?}");
    }

    #[test]
    fn same_winding_nesting_stays_solid_under_nonzero() {
        let doc = import_svg(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><path d="M1 1 H9 V9 H1 Z M3 3 H7 V7 H3 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(doc.objects.len(), 2);
        assert!(
            doc.objects.iter().all(|object| object.hole_of.is_none()),
            "same-winding contours were treated as holes"
        );
    }

    #[test]
    fn even_odd_marks_same_winding_nesting_as_a_hole() {
        let doc = import_svg(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><path fill-rule="evenodd" d="M1 1 H9 V9 H1 Z M3 3 H7 V7 H3 Z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(doc.objects.len(), 2);
        assert_eq!(
            doc.objects
                .iter()
                .filter(|object| object.hole_of.is_some())
                .count(),
            1,
            "even-odd did not mark the nested contour as a hole"
        );
    }

    #[test]
    fn an_island_inside_a_hole_stays_a_solid_shape() {
        let doc = import_svg(SVG_COMPOUND_ISLAND).unwrap();
        assert_eq!(doc.objects.len(), 3);
        let holes: Vec<_> = doc
            .objects
            .iter()
            .filter(|object| object.hole_of.is_some())
            .collect();
        assert_eq!(holes.len(), 1, "an island was mistaken for a hole");
        let island = doc
            .objects
            .iter()
            .find(|object| {
                object.hole_of.is_none()
                    && object
                        .bounds()
                        .is_some_and(|(min, _)| (min.x - 4.0).abs() < 1e-6)
            })
            .expect("the island is missing");
        let (min, max) = island.bounds().unwrap();
        assert!((min.x - 4.0).abs() < 1e-6);
        assert!((max.x - 6.0).abs() < 1e-6);
    }

    #[test]
    fn imports_path_with_arc() {
        let doc = import_svg(SVG_ARC).unwrap();
        assert_eq!(doc.objects.len(), 1);
        assert!(doc.objects[0].world_path().segment_count() >= 3);
    }

    #[test]
    fn parsing_can_be_cancelled() {
        let svg = r#"<svg width="10" height="10"><rect width="1" height="1"/></svg>"#;
        assert!(import_svg_report_cancellable(svg, &|| false).is_ok());
        let error = import_svg_report_cancellable(svg, &|| true).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn imports_text_alignment_and_spacing() {
        let svg = r#"<svg width="50" height="20"><text x="10" y="5" text-anchor="middle" letter-spacing="2" font-size="4">Hi</text></svg>"#;
        let report = import_svg_report(svg).unwrap();
        let object = report.document.objects.first().expect("text object");
        let crate::document::Shape::Text(text) = &object.shape else {
            panic!("expected text");
        };
        assert_eq!(text.align, crate::objects::TextAlign::Center);
        assert!(text.letter_spacing > 0.0);
    }

    #[test]
    fn imports_circle_and_line() {
        let doc = import_svg(SVG_SHAPES).unwrap();
        assert_eq!(doc.objects.len(), 2);
    }

    #[test]
    fn use_references_are_expanded_like_the_referenced_shape() {
        let direct = import_svg(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <rect x="20" y="30" width="10" height="5"/>
            </svg>"#,
        )
        .unwrap();
        let used = import_svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><rect id="r" width="10" height="5"/></defs>
                <use href="#r" x="20" y="30"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(direct.objects.len(), 1);
        assert_eq!(used.objects.len(), 1);
        let expected = direct.objects[0].bounds().unwrap();
        let actual = used.objects[0].bounds().unwrap();
        assert!(
            (expected.0.x - actual.0.x).abs() < 1e-6,
            "{expected:?} {actual:?}"
        );
        assert!(
            (expected.0.y - actual.0.y).abs() < 1e-6,
            "{expected:?} {actual:?}"
        );
        assert!(
            (expected.1.x - actual.1.x).abs() < 1e-6,
            "{expected:?} {actual:?}"
        );
        assert!(
            (expected.1.y - actual.1.y).abs() < 1e-6,
            "{expected:?} {actual:?}"
        );
    }

    #[test]
    fn external_use_resolves_a_sibling_svg() {
        let directory = std::env::temp_dir().join(format!("bw_svg_ext_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("external.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><rect id="r" x="1" y="1" width="4" height="4"/></defs></svg>"#,
        )
        .unwrap();
        let main = directory.join("main.svg");
        std::fs::write(
            &main,
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><use href="external.svg#r" x="5" y="6"/></svg>"#,
        )
        .unwrap();
        let report = import_svg_file_report(&main, &|| false).unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 6.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 10.0).abs() < 1e-6, "{max:?}");
        // SVG y is flipped in a 20 mm document: local 7..11 becomes 9..13.
        assert!((min.y - 9.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 13.0).abs() < 1e-6, "{max:?}");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn external_use_outside_the_folder_is_reported() {
        let root = std::env::temp_dir().join(format!("bw_svg_ext_out_{}", std::process::id()));
        let inner = root.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            root.join("outside.svg"),
            r#"<svg><rect id="r" width="4" height="4"/></svg>"#,
        )
        .unwrap();
        let main = inner.join("main.svg");
        std::fs::write(
            &main,
            r#"<svg width="10mm" height="10mm"><use href="../outside.svg#r"/></svg>"#,
        )
        .unwrap();
        let report = import_svg_file_report(&main, &|| false).unwrap();
        assert!(report.document.objects.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("outside the import folder")),
            "{:?}",
            report.warnings
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_use_reference_scales_its_symbol_view_box() {
        let doc = import_svg(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
                <defs><symbol id="s" viewBox="0 0 10 10">
                    <rect width="10" height="10"/>
                </symbol></defs>
                <use href="#s" x="20" y="30" width="20" height="20"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(doc.objects.len(), 1);
        let (min, max) = doc.objects[0].bounds().unwrap();
        assert!((min.x - 20.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 40.0).abs() < 1e-6, "{max:?}");
        // SVG y is flipped in a 100 mm document: 30..50 becomes 50..70.
        assert!((min.y - 50.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 70.0).abs() < 1e-6, "{max:?}");
    }

    #[test]
    fn use_references_to_missing_ids_are_reported() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm">
                <use href="#missing"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 0);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("missing #missing"))
        );
    }

    #[test]
    fn a_linear_gradient_uses_its_first_stop() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><linearGradient id="g">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </linearGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#ff0000")
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("approximated")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_pattern_uses_its_first_fill() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><pattern id="p"><rect width="2" height="2" fill="#00ff00"/></pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#00ff00")
        );
    }

    #[test]
    fn a_missing_paint_reference_is_reported() {
        let report = import_svg_report(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <rect x="0" y="0" width="10" height="10" fill="url(#missing)"/>
            </svg>"#,
        )
        .unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("#missing")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn reports_unsupported_elements() {
        let report = import_svg_report(SVG_USE).unwrap();
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn invalid_xml_is_an_error() {
        assert!(import_svg("<svg><rect></svg>").is_err());
    }

    #[test]
    fn parse_transform_scale() {
        let transform = parse_transform("translate(10,20) scale(2)");
        let point = transform.apply(Point2::new(1.0, 1.0));
        assert!((point.x - 12.0).abs() < 1e-9);
        assert!((point.y - 22.0).abs() < 1e-9);
    }

    #[test]
    fn length_units() {
        assert!((parse_length("25.4mm").unwrap().to_mm(DEFAULT_DPI).unwrap() - 25.4).abs() < 1e-9);
        assert!((parse_length("1in").unwrap().to_mm(DEFAULT_DPI).unwrap() - 25.4).abs() < 1e-9);
        assert!((parse_length("96px").unwrap().to_mm(DEFAULT_DPI).unwrap() - 25.4).abs() < 1e-9);
        assert!(parse_length("50%").unwrap().to_mm(DEFAULT_DPI).is_none());
    }

    #[test]
    fn illustrator_metadata_uses_72_dpi_for_unitless_lengths() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="72" height="36" viewBox="0 0 72 36"><!-- Generator: Adobe Illustrator 29.0.0 --><rect x="0" y="0" width="36" height="18"/></svg>"#;
        let doc = import_svg(svg).unwrap();
        assert!((doc.width_mm - 25.4).abs() < 1e-6, "{}", doc.width_mm);
        assert!((doc.height_mm - 12.7).abs() < 1e-6, "{}", doc.height_mm);
        let (min, max) = doc.objects[0].bounds().unwrap();
        assert!((max.x - min.x - 12.7).abs() < 1e-6);
    }

    #[test]
    fn unitless_lengths_default_to_96_dpi() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="10" height="10"/></svg>"#;
        let doc = import_svg(svg).unwrap();
        assert!((doc.width_mm - 25.4).abs() < 1e-6, "{}", doc.width_mm);
        let (min, max) = doc.objects[0].bounds().unwrap();
        assert!((max.x - min.x - 2.645_833).abs() < 1e-6);
    }

    #[test]
    fn style_is_read() {
        let doc = import_svg(SVG_STYLE).unwrap();
        let style = &doc.objects[0].style;
        assert!(style.stroke.is_none());
        assert_eq!(style.fill.as_deref(), Some("#ff0000"));
        assert!((style.stroke_width_mm - 0.3).abs() < 1e-9);
    }

    #[test]
    fn text_element() {
        let doc = import_svg(SVG_TEXT).unwrap();
        match &doc.objects[0].shape {
            Shape::Text(text) => assert_eq!(text.content, "hello"),
            other => panic!("unexpected shape: {other:?}"),
        }
    }
}
