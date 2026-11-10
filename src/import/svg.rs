#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names
)]

use std::collections::{HashMap, HashSet};
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
) -> Vec<ExternalClip> {
    let Some(base) = base_dir else {
        warnings.push(format!(
            "<use> to the external reference '{file}#{id}' needs a file path"
        ));
        return Vec::new();
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
        return Vec::new();
    }
    let canonical = match std::fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(error) => {
            warnings.push(format!("<use> to '{file}#{id}' was skipped: {error}"));
            return Vec::new();
        }
    };
    let base_canonical = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    if !canonical.starts_with(&base_canonical) {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: outside the import folder"
        ));
        return Vec::new();
    }
    if depth >= 8 {
        warnings.push("<use> nesting is too deep".to_string());
        return Vec::new();
    }
    if std::fs::metadata(&canonical).map_or(0, |metadata| metadata.len()) > MAX_SVG_BYTES {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: the file is too large"
        ));
        return Vec::new();
    }
    let Ok(source) = std::fs::read_to_string(&canonical) else {
        warnings.push(format!(
            "<use> to '{file}#{id}' was skipped: cannot read the file"
        ));
        return Vec::new();
    };
    let Ok(external) = parse_tree(&source) else {
        warnings.push(format!("<use> to '{file}#{id}' was skipped: invalid SVG"));
        return Vec::new();
    };
    let mut external_ids = HashMap::new();
    collect_ids(&external, &mut external_ids);
    let Some(target) = external_ids.get(id).copied() else {
        warnings.push(format!("<use> to '{file}#{id}' was skipped: missing #{id}"));
        return Vec::new();
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

    let mut resolved = Vec::new();
    for (attribute, expected, label) in [
        ("clip-path", "clipPath", "clip path"),
        ("mask", "mask", "mask"),
    ] {
        let Some(value) = node.attr(attribute) else {
            continue;
        };
        let id = value
            .trim()
            .strip_prefix("url(#")
            .and_then(|value| value.strip_suffix(')'))
            .unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let Some(target) = external_ids.get(id) else {
            continue;
        };
        if target.name != expected {
            continue;
        }
        let mut contours: Vec<Vec<(f64, f64)>> = Vec::new();
        for child in &target.children {
            contours.extend(clip_contours(child, transform));
        }
        let contours = weld_contours(contours);
        if !contours.is_empty() {
            resolved.push((label.to_string(), id.to_string(), contours));
        }
    }
    resolved
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
    let objects_before = doc.objects.len();

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
                        let clip_transform = transform.compose(&offset);
                        let transform =
                            clip_transform.compose(&use_view_box_transform(target, node));
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
                        apply_filter(node, transform, doc, objects_before, ids, warnings);
                        apply_clip(
                            node,
                            clip_transform,
                            doc,
                            objects_before,
                            ids,
                            &[],
                            warnings,
                        );
                    }
                }
            } else if let Some((file, id)) = href.split_once('#') {
                let clip_transform = transform.compose(&Transform2::translate(
                    node.number("x").unwrap_or(0.0),
                    node.number("y").unwrap_or(0.0),
                ));
                let external = expand_external_use(
                    node, file, id, transform, &style, doc, layer_id, warnings, depth, dpi,
                    base_dir,
                );
                apply_filter(node, clip_transform, doc, objects_before, ids, warnings);
                apply_clip(
                    node,
                    clip_transform,
                    doc,
                    objects_before,
                    ids,
                    &external,
                    warnings,
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
        "text" => import_text(node, transform, &style, doc, layer_id, warnings),
        "image" => import_image(node, transform, &style, doc, layer_id, dpi),
        other => warnings.push(format!("unsupported SVG element: <{other}>")),
    }
    if matches!(
        node.name.as_str(),
        "g" | "a"
            | "svg"
            | "rect"
            | "circle"
            | "ellipse"
            | "line"
            | "polyline"
            | "polygon"
            | "path"
            | "text"
            | "image"
    ) {
        apply_filter(node, transform, doc, objects_before, ids, warnings);
        apply_pattern(
            node,
            doc,
            objects_before,
            ids,
            warnings,
            depth,
            dpi,
            base_dir,
            layer_id,
        );
        apply_gradient(node, doc, objects_before, ids, warnings, layer_id);
        apply_clip(node, transform, doc, objects_before, ids, &[], warnings);
    }
}

/// Expand a linear gradient fill into solid colour bands clipped to the shape.
#[allow(clippy::too_many_arguments)]
fn apply_gradient(
    node: &Node,
    doc: &mut Document,
    start: usize,
    ids: &HashMap<String, &Node>,
    warnings: &mut Vec<String>,
    layer_id: &str,
) {
    let Some(id) = node.attr("data-gradient-fill") else {
        return;
    };
    let Some(gradient) = ids.get(id) else {
        return;
    };
    if gradient.name == "radialGradient" {
        apply_radial_gradient(doc, start, gradient, id, ids, warnings, layer_id);
        return;
    }
    if gradient.name != "linearGradient" {
        return;
    }
    let stops = gradient_stops(gradient, ids, 0);
    if stops.len() < 2 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    if doc.objects.len() != start + 1 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let (path, object_transform, local_bounds) = {
        let shape = &doc.objects[start];
        (shape.world_path(), shape.transform, shape.shape.bounds())
    };
    if !path.closed {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let Some((local_min, local_max)) = local_bounds else {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    };
    let object_bounding = gradient
        .attr("gradientUnits")
        .is_none_or(|units| units.eq_ignore_ascii_case("objectBoundingBox"));
    let (base, target_min, target_max) = if object_bounding {
        let local_width = local_max.x - local_min.x;
        let local_height = local_max.y - local_min.y;
        if local_width <= 0.0 || local_height <= 0.0 {
            fallback_gradient(doc, start, gradient, id, ids, warnings);
            return;
        }
        (
            object_transform
                .compose(&Transform2::translate(local_min.x, local_min.y))
                .compose(&Transform2::scale(local_width, local_height)),
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )
    } else {
        (object_transform, local_min, local_max)
    };
    let x1 = gradient_length(gradient, "x1", object_bounding).unwrap_or(0.0);
    let y1 = gradient_length(gradient, "y1", object_bounding).unwrap_or(0.0);
    let x2 = gradient_length(gradient, "x2", object_bounding).unwrap_or(1.0);
    let y2 = gradient_length(gradient, "y2", object_bounding).unwrap_or(0.0);
    let gradient_transform = gradient
        .attr("gradientTransform")
        .map(parse_transform)
        .unwrap_or_default();
    let first = gradient_transform.apply(Point2::new(x1, y1));
    let second = gradient_transform.apply(Point2::new(x2, y2));
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    let length = dx.hypot(dy);
    if length <= 1e-9 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let unit_x = dx / length;
    let unit_y = dy / length;
    let side_x = -unit_y;
    let side_y = unit_x;
    let corners = [
        target_min,
        Point2::new(target_max.x, target_min.y),
        Point2::new(target_min.x, target_max.y),
        target_max,
    ];
    let mut min_t = f64::INFINITY;
    let mut max_t = f64::NEG_INFINITY;
    let mut min_s = f64::INFINITY;
    let mut max_s = f64::NEG_INFINITY;
    for corner in corners {
        let relative_x = corner.x - first.x;
        let relative_y = corner.y - first.y;
        let along = relative_x * unit_x + relative_y * unit_y;
        let across = relative_x * side_x + relative_y * side_y;
        min_t = min_t.min(along);
        max_t = max_t.max(along);
        min_s = min_s.min(across);
        max_s = max_s.max(across);
    }
    let span = max_t - min_t;
    if span <= 1e-9 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let physical_length = base.apply(first).distance(base.apply(second));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bands = (physical_length.ceil() as usize).clamp(16, 64);
    let band_start = doc.objects.len();
    for band in 0..bands {
        #[allow(clippy::cast_precision_loss)]
        let t0 = min_t + span * band as f64 / bands as f64;
        #[allow(clippy::cast_precision_loss)]
        let t1 = min_t + span * (band + 1) as f64 / bands as f64;
        let parameter = (f64::midpoint(t0, t1) / length).clamp(0.0, 1.0);
        let Some(color) = color_at(&stops, parameter) else {
            continue;
        };
        let points = [(t0, min_s), (t1, min_s), (t1, max_s), (t0, max_s)];
        let world: Vec<(f64, f64)> = points
            .iter()
            .map(|(along, across)| {
                let point = Point2::new(
                    first.x + unit_x * along + side_x * across,
                    first.y + unit_y * along + side_y * across,
                );
                let point = base.apply(point);
                (point.x, point.y)
            })
            .collect();
        let mut object = Object::new(layer_id, Shape::Path(Path::from_points(&world, true)));
        object.style.fill = Some(color);
        object.style.stroke = None;
        object.style.stroke_width_mm = 0.0;
        doc.add_object(object);
    }
    if doc.objects.len() > band_start {
        let contour: Vec<(f64, f64)> = path
            .flatten(0.1)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        clip_objects(doc, band_start, id, "gradient", &[contour], warnings);
        warnings.push(format!("gradient #{id} was expanded into {bands} band(s)"));
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_radial_gradient(
    doc: &mut Document,
    start: usize,
    gradient: &Node,
    id: &str,
    ids: &HashMap<String, &Node>,
    warnings: &mut Vec<String>,
    layer_id: &str,
) {
    let stops = gradient_stops(gradient, ids, 0);
    if stops.len() < 2 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    if doc.objects.len() != start + 1 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let (path, object_transform, local_bounds) = {
        let shape = &doc.objects[start];
        (shape.world_path(), shape.transform, shape.shape.bounds())
    };
    if !path.closed {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let Some((local_min, local_max)) = local_bounds else {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    };
    let object_bounding = gradient
        .attr("gradientUnits")
        .is_none_or(|units| units.eq_ignore_ascii_case("objectBoundingBox"));
    let base = if object_bounding {
        let local_width = local_max.x - local_min.x;
        let local_height = local_max.y - local_min.y;
        if local_width <= 0.0 || local_height <= 0.0 {
            fallback_gradient(doc, start, gradient, id, ids, warnings);
            return;
        }
        object_transform
            .compose(&Transform2::translate(local_min.x, local_min.y))
            .compose(&Transform2::scale(local_width, local_height))
    } else {
        object_transform
    };
    let cx = gradient_length(gradient, "cx", object_bounding).unwrap_or(0.5);
    let cy = gradient_length(gradient, "cy", object_bounding).unwrap_or(0.5);
    let radius = gradient_length(gradient, "r", object_bounding).unwrap_or(0.5);
    let fx = gradient_length(gradient, "fx", object_bounding).unwrap_or(cx);
    let fy = gradient_length(gradient, "fy", object_bounding).unwrap_or(cy);
    if (fx - cx).abs() > 1e-9 || (fy - cy).abs() > 1e-9 || radius <= 0.0 {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let gradient_transform = gradient
        .attr("gradientTransform")
        .map(parse_transform)
        .unwrap_or_default();
    let map = |point: Point2| base.apply(gradient_transform.apply(point));
    let physical_radius = map(Point2::new(cx, cy)).distance(map(Point2::new(cx + radius, cy)));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bands = (physical_radius.ceil() as usize).clamp(16, 64);
    let circle = |fraction: f64| -> Vec<(f64, f64)> {
        (0..48)
            .map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let angle = std::f64::consts::TAU * f64::from(index) / 48.0;
                let point = Point2::new(
                    cx + radius * fraction * angle.cos(),
                    cy + radius * fraction * angle.sin(),
                );
                let point = map(point);
                (point.x, point.y)
            })
            .collect()
    };
    let shape_contour: Vec<(f64, f64)> = path
        .flatten(0.1)
        .into_iter()
        .map(|point| (point.x, point.y))
        .collect();
    let outer = circle(1.0);
    if !shape_contains(&shape_contour, &outer) {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    }
    let Some(background_color) = color_at(&stops, 1.0) else {
        fallback_gradient(doc, start, gradient, id, ids, warnings);
        return;
    };
    let difference = crate::geometry::boolean::boolean_contours(
        crate::geometry::boolean::BooleanOp::Difference,
        &shape_contour,
        &outer,
    );
    if !difference.is_empty() {
        let paths: Vec<Path> = difference
            .iter()
            .map(|contour| Path::from_points(contour, true))
            .collect();
        let parents = crate::geometry::path::contour_parents_with_rule(&paths, true);
        let mut order: Vec<usize> = (0..paths.len()).collect();
        order.sort_by_key(|&index| parents[index].is_some());
        let mut object_ids: Vec<Option<String>> = vec![None; paths.len()];
        for index in order {
            let mut object = Object::new(layer_id, Shape::Path(paths[index].clone()));
            object.style.fill = Some(background_color.clone());
            object.style.stroke = None;
            object.style.stroke_width_mm = 0.0;
            if let Some(parent_id) = parents[index].and_then(|parent| object_ids[parent].clone()) {
                object.hole_of = Some(parent_id);
            }
            object_ids[index] = Some(doc.add_object(object).id.clone());
        }
    }
    for band in 0..bands {
        #[allow(clippy::cast_precision_loss)]
        let inner_fraction = band as f64 / bands as f64;
        #[allow(clippy::cast_precision_loss)]
        let outer_fraction = (band + 1) as f64 / bands as f64;
        let Some(color) = color_at(&stops, f64::midpoint(inner_fraction, outer_fraction)) else {
            continue;
        };
        let outer_points = circle(outer_fraction);
        let mut outer_object = Object::new(
            layer_id,
            Shape::Path(Path::from_points(&outer_points, true)),
        );
        outer_object.style.fill = Some(color.clone());
        outer_object.style.stroke = None;
        outer_object.style.stroke_width_mm = 0.0;
        let outer_id = doc.add_object(outer_object).id.clone();
        if band > 0 {
            let inner_points = circle(inner_fraction);
            let mut hole = Object::new(
                layer_id,
                Shape::Path(Path::from_points(&inner_points, true)),
            );
            hole.style.fill = Some(color);
            hole.style.stroke = None;
            hole.style.stroke_width_mm = 0.0;
            hole.hole_of = Some(outer_id);
            doc.add_object(hole);
        }
    }
    warnings.push(format!("gradient #{id} was expanded into {bands} band(s)"));
}

fn shape_contains(outer: &[(f64, f64)], inner: &[(f64, f64)]) -> bool {
    let overlap = intersect_polygons(inner, outer);
    let covered: f64 = overlap
        .iter()
        .map(|contour| crate::geometry::boolean::polygon_area(contour).abs())
        .sum();
    let area = crate::geometry::boolean::polygon_area(inner).abs();
    area > 0.0 && covered >= area * 0.999
}

fn gradient_length(node: &Node, name: &str, fraction: bool) -> Option<f64> {
    let value = node.attr(name)?;
    let value = value.trim();
    if fraction {
        if let Some(percent) = value.strip_suffix('%') {
            percent
                .trim()
                .parse::<f64>()
                .ok()
                .map(|value| value / 100.0)
        } else {
            value.parse::<f64>().ok()
        }
    } else {
        node.number(name)
    }
}

fn fallback_gradient(
    doc: &mut Document,
    start: usize,
    gradient: &Node,
    id: &str,
    ids: &HashMap<String, &Node>,
    warnings: &mut Vec<String>,
) {
    let Some(color) = gradient_color(gradient, ids, 0) else {
        warnings.push(format!("gradient #{id} has no usable stops"));
        return;
    };
    if let Some(object) = doc.objects.get_mut(start) {
        object.style.fill = Some(color);
    }
    warnings.push(format!("gradient #{id} was approximated by one colour"));
}

/// Expand a `userSpaceOnUse` pattern fill into tiled geometry clipped to the
/// shape. Unsupported patterns fall back to the dominant representative colour.
#[allow(clippy::too_many_arguments)]
fn apply_pattern(
    node: &Node,
    doc: &mut Document,
    start: usize,
    ids: &HashMap<String, &Node>,
    warnings: &mut Vec<String>,
    depth: usize,
    dpi: f64,
    base_dir: Option<&FsPath>,
    layer_id: &str,
) {
    let Some(id) = node.attr("data-pattern-fill") else {
        return;
    };
    let Some(pattern) = ids.get(id) else {
        return;
    };
    if pattern.name != "pattern" {
        return;
    }
    if depth > 4 {
        fallback_pattern(doc, start, pattern, id, "is nested too deeply", warnings);
        return;
    }
    if pattern.children.is_empty() {
        fallback_pattern(doc, start, pattern, id, "has no children", warnings);
        return;
    }
    let object_bounding = pattern
        .attr("patternUnits")
        .is_none_or(|units| units.eq_ignore_ascii_case("objectBoundingBox"));
    let (Some(width), Some(height)) = (
        pattern_length(pattern, "width", object_bounding),
        pattern_length(pattern, "height", object_bounding),
    ) else {
        fallback_pattern(doc, start, pattern, id, "has no tile size", warnings);
        return;
    };
    if width <= 0.0 || height <= 0.0 {
        fallback_pattern(doc, start, pattern, id, "has a non-positive tile", warnings);
        return;
    }
    if doc.objects.len() != start + 1 {
        fallback_pattern(
            doc,
            start,
            pattern,
            id,
            "does not cover one shape",
            warnings,
        );
        return;
    }
    let (path, object_transform, local_bounds) = {
        let shape = &doc.objects[start];
        (shape.world_path(), shape.transform, shape.shape.bounds())
    };
    if !path.closed {
        fallback_pattern(doc, start, pattern, id, "has no closed outline", warnings);
        return;
    }
    let Some((local_min, local_max)) = local_bounds else {
        fallback_pattern(doc, start, pattern, id, "has no bounds", warnings);
        return;
    };
    let (base_transform, target_min, target_max) = if object_bounding {
        let local_width = local_max.x - local_min.x;
        let local_height = local_max.y - local_min.y;
        if local_width <= 0.0 || local_height <= 0.0 {
            fallback_pattern(doc, start, pattern, id, "has empty bounds", warnings);
            return;
        }
        (
            object_transform
                .compose(&Transform2::translate(local_min.x, local_min.y))
                .compose(&Transform2::scale(local_width, local_height)),
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
        )
    } else {
        (object_transform, local_min, local_max)
    };
    let pattern_transform = pattern
        .attr("patternTransform")
        .map(parse_transform)
        .unwrap_or_default();
    let Some(pattern_inverse) = pattern_transform.invert() else {
        fallback_pattern(
            doc,
            start,
            pattern,
            id,
            "has no inverse pattern transform",
            warnings,
        );
        return;
    };
    let corners = [
        target_min,
        Point2::new(target_max.x, target_min.y),
        Point2::new(target_min.x, target_max.y),
        target_max,
    ]
    .map(|point| pattern_inverse.apply(point));
    let mut pattern_min_x = f64::INFINITY;
    let mut pattern_min_y = f64::INFINITY;
    let mut pattern_max_x = f64::NEG_INFINITY;
    let mut pattern_max_y = f64::NEG_INFINITY;
    for point in corners {
        pattern_min_x = pattern_min_x.min(point.x);
        pattern_min_y = pattern_min_y.min(point.y);
        pattern_max_x = pattern_max_x.max(point.x);
        pattern_max_y = pattern_max_y.max(point.y);
    }
    let x = pattern_length(pattern, "x", object_bounding).unwrap_or(0.0);
    let y = pattern_length(pattern, "y", object_bounding).unwrap_or(0.0);
    let start_x = x + ((pattern_min_x - x) / width).floor() * width;
    let start_y = y + ((pattern_min_y - y) / height).floor() * height;
    let style = SvgStyle::from_node(pattern);
    let pattern_start = doc.objects.len();
    let tile_base = base_transform.compose(&pattern_transform);
    let mut tiles = 0usize;
    let mut overflow = false;
    let mut column = start_x;
    'outer: while column < pattern_max_x + width {
        let mut row = start_y;
        while row < pattern_max_y + height {
            if tiles >= 2048 {
                overflow = true;
                break 'outer;
            }
            tiles += 1;
            let tile = tile_base.compose(&Transform2::translate(column, row));
            for child in &pattern.children {
                process_node(
                    child,
                    tile,
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
            row += height;
        }
        column += width;
    }
    if overflow {
        doc.objects.truncate(pattern_start);
        fallback_pattern(doc, start, pattern, id, "has too many tiles", warnings);
        return;
    }
    if doc.objects.len() > pattern_start {
        let contour: Vec<(f64, f64)> = path
            .flatten(0.1)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        clip_objects(doc, pattern_start, id, "pattern", &[contour], warnings);
        warnings.push(format!("pattern #{id} was expanded into {tiles} tile(s)"));
    }
}

fn pattern_length(node: &Node, name: &str, fraction: bool) -> Option<f64> {
    let value = node.attr(name)?;
    let value = value.trim();
    if fraction {
        if let Some(percent) = value.strip_suffix('%') {
            percent
                .trim()
                .parse::<f64>()
                .ok()
                .map(|value| value / 100.0)
        } else {
            value.parse::<f64>().ok()
        }
    } else {
        node.number(name)
    }
}

fn fallback_pattern(
    doc: &mut Document,
    start: usize,
    pattern: &Node,
    id: &str,
    reason: &str,
    warnings: &mut Vec<String>,
) {
    let Some(color) = pattern_color(pattern, 0) else {
        warnings.push(format!("pattern #{id} {reason}"));
        return;
    };
    if let Some(object) = doc.objects.get_mut(start) {
        object.style.fill = Some(color);
    }
    warnings.push(format!(
        "pattern #{id} {reason}; approximated by one colour"
    ));
}

/// Apply the subset of SVG filters that a vector document can represent.
///
/// A single `feOffset` becomes a translation and a single
/// `feColorMatrix type="saturate"` is applied to solid fills and strokes.
/// Everything else falls back to the reported-and-unfiltered behaviour.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn apply_filter(
    node: &Node,
    transform: Transform2,
    doc: &mut Document,
    start: usize,
    ids: &HashMap<String, &Node>,
    warnings: &mut Vec<String>,
) {
    let Some(value) = node.attr("filter") else {
        return;
    };
    if value.trim().eq_ignore_ascii_case("none") {
        return;
    }
    let id = value
        .trim()
        .strip_prefix("url(#")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or_default();
    if id.is_empty() {
        warn_filter(node, ids, warnings);
        return;
    }
    let Some(target) = ids.get(id) else {
        warn_filter(node, ids, warnings);
        return;
    };
    if target.name != "filter" || start >= doc.objects.len() {
        warn_filter(node, ids, warnings);
        return;
    }
    let primitives: Vec<&Node> = target
        .children
        .iter()
        .filter(|child| child.name.starts_with("fe"))
        .collect();
    if primitives.len() == 1 {
        let primitive = primitives[0];
        if primitive.name == "feOffset" {
            let source = primitive.attr("in").unwrap_or("SourceGraphic");
            if !source.eq_ignore_ascii_case("SourceGraphic")
                && !source.eq_ignore_ascii_case("SourceAlpha")
            {
                warn_filter(node, ids, warnings);
                return;
            }
            let Some(dx) = filter_length(primitive, "dx") else {
                warn_filter(node, ids, warnings);
                return;
            };
            let Some(dy) = filter_length(primitive, "dy") else {
                warn_filter(node, ids, warnings);
                return;
            };
            let (dx, dy) = (
                transform.a * dx + transform.c * dy,
                transform.b * dx + transform.d * dy,
            );
            for object in &mut doc.objects[start..] {
                object.translate(dx, dy);
            }
            warnings.push(format!(
                "filter #{id} (feOffset) was applied as a translation"
            ));
            return;
        }
        if primitive.name == "feColorMatrix"
            && primitive
                .attr("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("saturate"))
        {
            let source = primitive.attr("in").unwrap_or("SourceGraphic");
            if !source.eq_ignore_ascii_case("SourceGraphic") {
                warn_filter(node, ids, warnings);
                return;
            }
            let amount = primitive.number("values").unwrap_or(1.0);
            if doc.objects[start..]
                .iter()
                .any(|object| matches!(object.shape, Shape::Image(_)))
            {
                warn_filter(node, ids, warnings);
                return;
            }
            for object in &mut doc.objects[start..] {
                saturate_object(object, amount);
            }
            warnings.push(format!(
                "filter #{id} (feColorMatrix saturate) was applied to the colours"
            ));
            return;
        }
    }
    warn_filter(node, ids, warnings);
}

/// Parse a numeric filter length, treating a missing value as zero.
fn filter_length(node: &Node, name: &str) -> Option<f64> {
    match node.attr(name) {
        None => Some(0.0),
        Some(value) => value.trim().parse::<f64>().ok(),
    }
}

/// Apply the SVG `saturate` colour matrix to an object's solid colours.
fn saturate_object(object: &mut Object, amount: f64) {
    let amount = amount.clamp(0.0, 1.0);
    let convert = |value: &str| -> Option<String> {
        let (red, green, blue) = crate::color::parse_hex_color(value)?;
        let red = f64::from(red);
        let green = f64::from(green);
        let blue = f64::from(blue);
        let output = |a: f64, b: f64, c: f64| {
            (a * red + b * green + c * blue).round().clamp(0.0, 255.0) as u8
        };
        Some(crate::color::to_hex(
            output(
                0.213 + 0.787 * amount,
                0.715 - 0.715 * amount,
                0.072 - 0.072 * amount,
            ),
            output(
                0.213 - 0.213 * amount,
                0.715 + 0.285 * amount,
                0.072 - 0.072 * amount,
            ),
            output(
                0.213 - 0.213 * amount,
                0.715 - 0.715 * amount,
                0.072 + 0.928 * amount,
            ),
        ))
    };
    if let Some(fill) = object.style.fill.as_deref().and_then(convert) {
        object.style.fill = Some(fill);
    }
    if let Some(stroke) = object.style.stroke.as_deref().and_then(convert) {
        object.style.stroke = Some(stroke);
    }
    if let Some(color) = object.color.as_deref().and_then(convert) {
        object.color = Some(color);
    }
}

/// Report an SVG `filter` without applying it.
///
/// Filters describe raster and visual effects; the document stores solid
/// fills and plain geometry, so the element is imported unfiltered and the
/// primitive names are listed in the warning.
fn warn_filter(node: &Node, ids: &HashMap<String, &Node>, warnings: &mut Vec<String>) {
    let Some(value) = node.attr("filter") else {
        return;
    };
    if value.trim().eq_ignore_ascii_case("none") {
        return;
    }
    let id = value
        .trim()
        .strip_prefix("url(#")
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or_default();
    if id.is_empty() {
        warnings.push(format!(
            "filter '{value}' is not a local reference; <{}> is imported unfiltered",
            node.name
        ));
        return;
    }
    let Some(target) = ids.get(id) else {
        warnings.push(format!(
            "filter references missing #{id}; <{}> is imported unfiltered",
            node.name
        ));
        return;
    };
    if target.name != "filter" {
        warnings.push(format!(
            "#{id} is not a filter; <{}> is imported unfiltered",
            node.name
        ));
        return;
    }
    let primitives: Vec<&str> = target
        .children
        .iter()
        .map(|child| child.name.as_str())
        .filter(|name| name.starts_with("fe"))
        .collect();
    if primitives.is_empty() {
        warnings.push(format!(
            "filter #{id} has no primitives; <{}> is imported unfiltered",
            node.name
        ));
    } else {
        warnings.push(format!(
            "filter #{id} ({}) is ignored; <{}> is imported unfiltered",
            primitives.join(", "),
            node.name
        ));
    }
}

/// Apply a `clip-path="url(#id)"` on one shape to the objects it produced.
///
/// The clip contours are flattened and intersected with each closed path; the
/// result is written as straight segments. Text, images and open paths are
/// reported and left unclipped. Group clips are not expanded.
fn apply_clip(
    node: &Node,
    transform: Transform2,
    doc: &mut Document,
    start: usize,
    ids: &HashMap<String, &Node>,
    external: &[ExternalClip],
    warnings: &mut Vec<String>,
) {
    for (attribute, expected, label) in [
        ("clip-path", "clipPath", "clip path"),
        ("mask", "mask", "mask"),
    ] {
        let Some(value) = node.attr(attribute) else {
            continue;
        };
        let id = value
            .trim()
            .strip_prefix("url(#")
            .and_then(|value| value.strip_suffix(')'))
            .unwrap_or_default();
        if id.is_empty() {
            warnings.push(format!("{label} '{value}' is not a local reference"));
            continue;
        }
        let (label, contours) = if let Some(target) = ids.get(id) {
            if target.name != expected {
                warnings.push(format!("#{id} is not a {label}"));
                continue;
            }
            let mut contours: Vec<Vec<(f64, f64)>> = Vec::new();
            for child in &target.children {
                contours.extend(clip_contours(child, transform));
            }
            let contours = weld_contours(contours);
            if contours.is_empty() {
                warnings.push(format!("{label} #{id} has no usable shape"));
                continue;
            }
            (label, contours)
        } else if let Some((external_label, _, external_contours)) = external
            .iter()
            .find(|(_, external_id, _)| external_id == id)
        {
            if external_contours.is_empty() {
                warnings.push(format!("{external_label} #{id} has no usable shape"));
                continue;
            }
            (external_label.as_str(), external_contours.clone())
        } else {
            warnings.push(format!("{label} references missing #{id}"));
            continue;
        };
        if label == "mask" {
            warnings.push(format!(
                "mask #{id} was approximated as a clip path (white shapes only)"
            ));
        }
        clip_objects(doc, start, id, label, &contours, warnings);
    }
}

fn clip_objects(
    doc: &mut Document,
    start: usize,
    id: &str,
    label: &str,
    contours: &[Vec<(f64, f64)>],
    warnings: &mut Vec<String>,
) {
    let objects: Vec<Object> = doc.objects.split_off(start);
    let mut replaced = Vec::new();
    let mut applied = false;
    for mut object in objects {
        if let Shape::Text(text) = &object.shape {
            let text = text.clone();
            clip_text_object(
                doc,
                &mut replaced,
                &object,
                &text,
                contours,
                label,
                id,
                warnings,
                &mut applied,
            );
            continue;
        }
        if matches!(object.shape, Shape::Image(_)) {
            let inverse = object.transform.invert();
            let name = object.name.clone();
            if let Shape::Image(image) = &mut object.shape {
                clip_image_object(
                    image,
                    inverse,
                    &name,
                    contours,
                    label,
                    id,
                    warnings,
                    &mut applied,
                );
            }
            replaced.push(object);
            continue;
        }
        let path = object.world_path();
        if !path.closed {
            applied = true;
            let points = path.flatten(0.1);
            let mut runs: Vec<Vec<Point2>> = Vec::new();
            let mut current: Vec<Point2> = Vec::new();
            for pair in points.windows(2) {
                let from = pair[0];
                let to = pair[1];
                let intervals = inside_intervals(from, to, contours);
                if intervals.is_empty() {
                    if current.len() >= 2 {
                        runs.push(std::mem::take(&mut current));
                    } else {
                        current.clear();
                    }
                    continue;
                }
                for (start, end) in intervals {
                    let p0 = Point2::new(
                        from.x + (to.x - from.x) * start,
                        from.y + (to.y - from.y) * start,
                    );
                    let p1 = Point2::new(
                        from.x + (to.x - from.x) * end,
                        from.y + (to.y - from.y) * end,
                    );
                    if current.last().is_some_and(|last| last.distance(p0) > 1e-6) {
                        if current.len() >= 2 {
                            runs.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                    }
                    if current.is_empty() {
                        current.push(p0);
                    }
                    current.push(p1);
                    if end < 1.0 - 1e-6 {
                        if current.len() >= 2 {
                            runs.push(std::mem::take(&mut current));
                        } else {
                            current.clear();
                        }
                    }
                }
            }
            if current.len() >= 2 {
                runs.push(current);
            }
            for run in runs {
                let mut open = Object::new(object.layer_id.clone(), Shape::Path(open_path(&run)))
                    .with_style(object.style.clone());
                open.name = format!("{} clipped", object.name);
                open.color.clone_from(&object.color);
                open.operation_id.clone_from(&object.operation_id);
                open.group_id.clone_from(&object.group_id);
                open.visible = object.visible;
                open.locked = object.locked;
                doc.add_object(open);
            }
            continue;
        }
        let mut pieces = vec![
            path.flatten(0.1)
                .into_iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        ];
        for clip in contours {
            pieces = pieces
                .into_iter()
                .flat_map(|piece| intersect_polygons(&piece, clip))
                .collect();
        }
        if pieces.is_empty() {
            eprintln!(
                "CLIP EMPTY object={} bounds={:?} contours={contours:?}",
                object.name,
                object.bounds()
            );
            // `boolean_contours` can return nothing when the subject is fully
            // inside the clip; keep it then, and drop it when fully outside.
            let flat = path.flatten(0.1);
            let inside = !flat.is_empty()
                && contours.iter().any(|contour| {
                    let clip_min_x = contour.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                    let clip_min_y = contour.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                    let clip_max_x = contour
                        .iter()
                        .map(|p| p.0)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let clip_max_y = contour
                        .iter()
                        .map(|p| p.1)
                        .fold(f64::NEG_INFINITY, f64::max);
                    flat.iter().all(|point| {
                        point.x >= clip_min_x - 1e-6
                            && point.y >= clip_min_y - 1e-6
                            && point.x <= clip_max_x + 1e-6
                            && point.y <= clip_max_y + 1e-6
                    })
                });
            if inside {
                replaced.push(object);
            }
            applied = true;
            continue;
        }
        applied = true;
        let resolved: Vec<Path> = pieces
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
        let mut new_ids: Vec<Option<String>> = vec![None; resolved.len()];
        for index in order {
            let mut clipped = Object::new(
                object.layer_id.clone(),
                Shape::Path(resolved[index].clone()),
            )
            .with_style(object.style.clone());
            clipped.name = if parents[index].is_some() {
                "Hole".to_string()
            } else {
                format!("{} clipped", object.name)
            };
            clipped.color.clone_from(&object.color);
            clipped.operation_id.clone_from(&object.operation_id);
            clipped.group_id.clone_from(&object.group_id);
            clipped.visible = object.visible;
            clipped.locked = object.locked;
            if let Some(parent_id) = parents[index].and_then(|parent| new_ids[parent].clone()) {
                clipped.hole_of = Some(parent_id);
            }
            new_ids[index] = Some(doc.add_object(clipped).id.clone());
        }
    }
    doc.objects.extend(replaced);
    if applied {
        warnings.push(format!("{label} #{id} was applied as straight segments"));
    }
}

/// Apply a clip to a text object by outlining it and clipping the outlines.
#[allow(clippy::too_many_arguments)]
fn clip_text_object(
    doc: &mut Document,
    replaced: &mut Vec<Object>,
    object: &Object,
    text: &TextObject,
    contours: &[Vec<(f64, f64)>],
    label: &str,
    id: &str,
    warnings: &mut Vec<String>,
    applied: &mut bool,
) {
    let paths = match crate::objects::text_to_paths(text) {
        Ok(paths) if !paths.is_empty() => paths,
        Ok(_) => {
            warnings.push(format!(
                "{label} #{id} was not applied to text '{}': no outlines",
                object.name
            ));
            replaced.push(object.clone());
            return;
        }
        Err(error) => {
            warnings.push(format!(
                "{label} #{id} was not applied to text '{}': {error}",
                object.name
            ));
            replaced.push(object.clone());
            return;
        }
    };
    let world_paths: Vec<Path> = paths
        .iter()
        .map(|path| path.transform(&object.transform))
        .collect();
    let mut pieces: Vec<Vec<(f64, f64)>> = Vec::new();
    for path in &world_paths {
        if !path.closed {
            continue;
        }
        let points: Vec<(f64, f64)> = path
            .flatten(0.1)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        let mut current = vec![points];
        for clip in contours {
            current = current
                .into_iter()
                .flat_map(|piece| intersect_polygons(&piece, clip))
                .collect();
        }
        pieces.extend(current);
    }
    if pieces.is_empty() {
        let flat: Vec<Point2> = world_paths
            .iter()
            .flat_map(|path| path.flatten(0.1))
            .collect();
        let inside = !flat.is_empty()
            && contours.iter().any(|contour| {
                flat.iter().all(|point| {
                    crate::geometry::boolean::point_in_polygon(point.x, point.y, contour)
                })
            });
        if inside {
            replaced.push(object.clone());
        }
        *applied = true;
        return;
    }
    *applied = true;
    let resolved: Vec<Path> = pieces
        .iter()
        .map(|contour| {
            let points: Vec<Point2> = contour.iter().map(|(x, y)| Point2::new(*x, *y)).collect();
            crate::vectorize::to_path(&points)
        })
        .collect();
    let parents = crate::geometry::path::contour_parents(&resolved);
    let mut order: Vec<usize> = (0..resolved.len()).collect();
    order.sort_by_key(|&index| parents[index].is_some());
    let mut new_ids: Vec<Option<String>> = vec![None; resolved.len()];
    for index in order {
        let mut clipped = Object::new(
            object.layer_id.clone(),
            Shape::Path(resolved[index].clone()),
        )
        .with_style(object.style.clone());
        clipped.name = if parents[index].is_some() {
            "Hole".to_string()
        } else {
            format!("{} clipped", object.name)
        };
        clipped.color.clone_from(&object.color);
        clipped.operation_id.clone_from(&object.operation_id);
        clipped.group_id.clone_from(&object.group_id);
        clipped.visible = object.visible;
        clipped.locked = object.locked;
        if let Some(parent_id) = parents[index].and_then(|parent| new_ids[parent].clone()) {
            clipped.hole_of = Some(parent_id);
        }
        new_ids[index] = Some(doc.add_object(clipped).id.clone());
    }
    warnings.push(format!(
        "{label} #{id} was applied to text '{}' by converting it to outlines",
        object.name
    ));
}

/// Apply a clip to an image by storing it as a vector mask in image-local mm.
#[allow(clippy::too_many_arguments)]
fn clip_image_object(
    image: &mut ImageObject,
    inverse: Option<Transform2>,
    name: &str,
    contours: &[Vec<(f64, f64)>],
    label: &str,
    id: &str,
    warnings: &mut Vec<String>,
    applied: &mut bool,
) {
    let Some(inverse) = inverse else {
        warnings.push(format!(
            "{label} #{id} was not applied to image '{name}': the transform is not invertible"
        ));
        return;
    };
    if image.mask_invert {
        warnings.push(format!(
            "{label} #{id} was not combined with the inverted image mask on '{name}'"
        ));
        return;
    }
    let local: Vec<Vec<(f64, f64)>> = contours
        .iter()
        .map(|contour| {
            contour
                .iter()
                .map(|(x, y)| {
                    let point = inverse.apply(Point2::new(*x, *y));
                    (point.x, point.y)
                })
                .collect()
        })
        .collect();
    let polygons = if image.mask_paths.is_empty() {
        local
    } else {
        let mut result: Vec<Vec<(f64, f64)>> = image
            .mask_paths
            .iter()
            .filter(|path| path.closed)
            .map(|path| {
                path.flatten(0.2)
                    .into_iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>()
            })
            .collect();
        for clip in &local {
            result = result
                .into_iter()
                .flat_map(|piece| intersect_polygons(&piece, clip))
                .collect();
        }
        result
    };
    let width = image.width_mm();
    let height = image.height_mm();
    if polygons.is_empty() {
        if width > 0.0 && height > 0.0 {
            image.mask_paths = vec![Path::from_points(
                &[(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)],
                true,
            )];
            image.mask_invert = true;
            *applied = true;
            warnings.push(format!("{label} #{id} masked the whole image '{name}'"));
        } else {
            warnings.push(format!(
                "{label} #{id} was not applied to image '{name}': empty clip"
            ));
        }
        return;
    }
    image.mask_paths = polygons
        .into_iter()
        .filter(|polygon| polygon.len() >= 3)
        .map(|polygon| Path::from_points(&polygon, true))
        .collect();
    *applied = true;
    warnings.push(format!(
        "{label} #{id} was applied to image '{name}' as a vector mask"
    ));
}

/// Intersect two contours after matching their winding direction.
pub(crate) fn intersect_polygons(
    piece: &[(f64, f64)],
    clip: &[(f64, f64)],
) -> Vec<Vec<(f64, f64)>> {
    let mut oriented = clip.to_vec();
    if crate::geometry::boolean::polygon_area(piece).signum()
        != crate::geometry::boolean::polygon_area(&oriented).signum()
    {
        oriented.reverse();
    }
    let result = crate::geometry::boolean::boolean_contours(
        crate::geometry::boolean::BooleanOp::Intersection,
        piece,
        &oriented,
    );
    if !result.is_empty() || oriented.len() < 3 {
        return result;
    }
    let clip_min_x = oriented
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::INFINITY, f64::min);
    let clip_min_y = oriented
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::INFINITY, f64::min);
    let clip_max_x = oriented
        .iter()
        .map(|(x, _)| *x)
        .fold(f64::NEG_INFINITY, f64::max);
    let clip_max_y = oriented
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    let piece_inside = piece.iter().all(|(x, y)| {
        *x >= clip_min_x - 1e-9
            && *x <= clip_max_x + 1e-9
            && *y >= clip_min_y - 1e-9
            && *y <= clip_max_y + 1e-9
    });
    if piece_inside {
        return vec![piece.to_vec()];
    }
    // Shared edges and touching vertices can make the boolean walk return
    // nothing. A sub-micrometre inset breaks the tie without moving the clip
    // by a visible amount.
    let count = oriented.len() as f64;
    let cx = oriented.iter().map(|(x, _)| *x).sum::<f64>() / count;
    let cy = oriented.iter().map(|(_, y)| *y).sum::<f64>() / count;
    let inset: Vec<(f64, f64)> = oriented
        .iter()
        .map(|(x, y)| (cx + (x - cx) * 0.999_999, cy + (y - cy) * 0.999_999))
        .collect();
    crate::geometry::boolean::boolean_contours(
        crate::geometry::boolean::BooleanOp::Intersection,
        piece,
        &inset,
    )
}

/// Resolve overlapping contours with a scanline fill.
///
/// Rows are the vertex ordinates, so every band has linear edges and the
/// trapezoids are exact for polygonal input. The result uses straight
/// segments; it is the fallback for three or more partially overlapping
/// contours whose winding or fill rule cannot be welded pairwise.
pub(crate) fn scanline_fill(polygons: &[Vec<(f64, f64)>], even_odd: bool) -> Vec<Vec<(f64, f64)>> {
    struct Crossing {
        x: f64,
        direction: i32,
        edge: ScanlineEdge,
    }

    fn x_on_edge(edge: ScanlineEdge, y: f64) -> f64 {
        let (a, b) = edge;
        if (b.1 - a.1).abs() < 1e-12 {
            a.0
        } else {
            a.0 + (b.0 - a.0) * (y - a.1) / (b.1 - a.1)
        }
    }

    let mut rows: Vec<f64> = polygons
        .iter()
        .flat_map(|polygon| polygon.iter().map(|(_, y)| *y))
        .collect();
    rows.sort_by(f64::total_cmp);
    rows.dedup_by(|left, right| (*left - *right).abs() < 1e-9);

    let mut result = Vec::new();
    for window in rows.windows(2) {
        let (y0, y1) = (window[0], window[1]);
        if y1 - y0 < 1e-9 {
            continue;
        }
        let middle = f64::midpoint(y0, y1);
        let mut crossings: Vec<Crossing> = Vec::new();
        for polygon in polygons {
            for index in 0..polygon.len() {
                let a = polygon[index];
                let b = polygon[(index + 1) % polygon.len()];
                if (a.1 <= middle && b.1 > middle) || (b.1 <= middle && a.1 > middle) {
                    let t = (middle - a.1) / (b.1 - a.1);
                    crossings.push(Crossing {
                        x: a.0 + (b.0 - a.0) * t,
                        direction: if b.1 > a.1 { 1 } else { -1 },
                        edge: (a, b),
                    });
                }
            }
        }
        crossings.sort_by(|left, right| left.x.total_cmp(&right.x));

        let mut intervals: Vec<ScanlineInterval> = Vec::new();
        if even_odd {
            let mut index = 0;
            while index + 1 < crossings.len() {
                let left = &crossings[index];
                let right = &crossings[index + 1];
                if right.x - left.x > 1e-9 {
                    intervals.push((left.edge, right.edge));
                }
                index += 2;
            }
        } else {
            let mut winding = 0;
            let mut open: Option<usize> = None;
            for (index, crossing) in crossings.iter().enumerate() {
                let before = winding;
                winding += crossing.direction;
                if before == 0 && winding != 0 {
                    open = Some(index);
                } else if before != 0 && winding == 0 {
                    if let Some(start) = open.take()
                        && index > start
                        && crossing.x - crossings[start].x > 1e-9
                    {
                        intervals.push((crossings[start].edge, crossing.edge));
                    }
                }
            }
        }

        for (left, right) in intervals {
            let left_bottom = x_on_edge(left, y0);
            let right_bottom = x_on_edge(right, y0);
            let right_top = x_on_edge(right, y1);
            let left_top = x_on_edge(left, y1);
            let points = [
                (left_bottom, y0),
                (right_bottom, y0),
                (right_top, y1),
                (left_top, y1),
            ];
            let mut contour: Vec<(f64, f64)> = Vec::new();
            for point in points {
                if contour
                    .last()
                    .is_none_or(|last| (last.0 - point.0).hypot(last.1 - point.1) > 1e-9)
                {
                    contour.push(point);
                }
            }
            if contour.len() >= 3
                && (contour[0].0 - contour[contour.len() - 1].0)
                    .hypot(contour[0].1 - contour[contour.len() - 1].1)
                    < 1e-9
            {
                contour.pop();
            }
            if contour.len() >= 3 {
                result.push(contour);
            }
        }
    }
    result
}

/// Build an open path from a polyline.
fn open_path(points: &[Point2]) -> Path {
    let mut path = Path::new();
    for pair in points.windows(2) {
        path.push(Segment::Line {
            from: pair[0],
            to: pair[1],
        });
    }
    path
}

/// Parameters along a segment that lie inside any clip contour.
fn inside_intervals(from: Point2, to: Point2, contours: &[Vec<(f64, f64)>]) -> Vec<(f64, f64)> {
    let mut intervals: Vec<(f64, f64)> = contours
        .iter()
        .flat_map(|contour| polygon_intervals(from, to, contour))
        .collect();
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut()
            && start <= last.1 + 1e-9
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

/// Parameters along a segment where it lies inside a polygon.
#[allow(clippy::cast_precision_loss)]
fn polygon_intervals(from: Point2, to: Point2, polygon: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        return Vec::new();
    }
    let mut cuts = vec![0.0, 1.0];
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let ex = b.0 - a.0;
        let ey = b.1 - a.1;
        let path_cross_edge = dx * ey - dy * ex;
        if path_cross_edge.abs() < 1e-12 {
            continue;
        }
        let t = ((a.0 - from.x) * ey - (a.1 - from.y) * ex) / path_cross_edge;
        let s = ((a.0 - from.x) * dy - (a.1 - from.y) * dx) / path_cross_edge;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
            cuts.push(t);
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|left, right| (*left - *right).abs() < 1e-9);
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for window in cuts.windows(2) {
        let middle = f64::midpoint(window[0], window[1]);
        let point = Point2::new(from.x + dx * middle, from.y + dy * middle);
        if crate::geometry::boolean::point_in_polygon(point.x, point.y, polygon) {
            if let Some(last) = intervals.last_mut()
                && (last.1 - window[0]).abs() < 1e-9
            {
                last.1 = window[1];
                continue;
            }
            intervals.push((window[0], window[1]));
        }
    }
    intervals
}

/// Flatten the geometry of one clipPath child in document coordinates.
fn clip_contours(node: &Node, transform: Transform2) -> Vec<Vec<(f64, f64)>> {
    let local = node
        .attr("transform")
        .map(parse_transform)
        .unwrap_or_default();
    let transform = transform.compose(&local);
    let points = |points: Vec<Point2>| {
        points
            .into_iter()
            .map(|point| {
                let point = transform.apply(point);
                (point.x, point.y)
            })
            .collect::<Vec<_>>()
    };
    match node.name.as_str() {
        "rect" => {
            let x = node.number("x").unwrap_or(0.0);
            let y = node.number("y").unwrap_or(0.0);
            let width = node.number("width").unwrap_or(0.0);
            let height = node.number("height").unwrap_or(0.0);
            if width <= 0.0 || height <= 0.0 {
                return Vec::new();
            }
            vec![points(vec![
                Point2::new(x, y),
                Point2::new(x + width, y),
                Point2::new(x + width, y + height),
                Point2::new(x, y + height),
            ])]
        }
        "circle" | "ellipse" => {
            let cx = node.number("cx").unwrap_or(0.0);
            let cy = node.number("cy").unwrap_or(0.0);
            let (rx, ry) = if node.name == "circle" {
                let radius = node.number("r").unwrap_or(0.0);
                (radius, radius)
            } else {
                (
                    node.number("rx").unwrap_or(0.0),
                    node.number("ry").unwrap_or(0.0),
                )
            };
            if rx <= 0.0 || ry <= 0.0 {
                return Vec::new();
            }
            let mut contour = Vec::with_capacity(32);
            for index in 0..32 {
                let angle = std::f64::consts::TAU * f64::from(index) / 32.0;
                contour.push(Point2::new(cx + rx * angle.cos(), cy + ry * angle.sin()));
            }
            vec![points(contour)]
        }
        "path" => {
            let Some(data) = node.attr("d") else {
                return Vec::new();
            };
            let Ok(path) = kurbo::BezPath::from_svg(data) else {
                return Vec::new();
            };
            bezpath_to_paths(&path)
                .into_iter()
                .map(|path| points(path.flatten(0.1)))
                .filter(|contour| contour.len() >= 3)
                .collect()
        }
        "polygon" => {
            let Some(raw) = node.attr("points") else {
                return Vec::new();
            };
            let values: Vec<f64> = raw
                .split(|character: char| character == ',' || character.is_whitespace())
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse::<f64>().ok())
                .collect();
            if values.len() < 6 || values.len() % 2 != 0 {
                return Vec::new();
            }
            let contour: Vec<Point2> = values
                .chunks_exact(2)
                .map(|pair| Point2::new(pair[0], pair[1]))
                .collect();
            vec![points(contour)]
        }
        _ => Vec::new(),
    }
}

/// Flattened clip contours in document coordinates.
type ClipContours = Vec<Vec<(f64, f64)>>;
/// One clip or mask resolved from an external SVG: label, id, contours.
type ExternalClip = (String, String, ClipContours);
/// One polygon edge used by the scanline fill.
type ScanlineEdge = ((f64, f64), (f64, f64));
/// Left and right edges of one scanline interval.
type ScanlineInterval = (ScanlineEdge, ScanlineEdge);

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
pub(crate) fn contours_partially_overlap(first: &[(f64, f64)], second: &[(f64, f64)]) -> bool {
    use crate::geometry::boolean::polygon_area;

    let overlap = intersect_polygons(first, second);
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
    if paths.len() > 2 {
        eprintln!(
            "3+ paths={} closed={} fill={:?} even={}",
            paths.len(),
            paths.iter().all(|p| p.closed),
            style.fill,
            style.even_odd()
        );
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
            let trapezoids = scanline_fill(&polygons, style.even_odd());
            if !trapezoids.is_empty() {
                warnings.push(
                    "partially overlapping contours were resolved as straight scanline fills"
                        .to_string(),
                );
                for contour in trapezoids {
                    let path = Path::from_points(&contour, true);
                    let mut object = Object::new(layer_id, Shape::Path(path))
                        .with_transform(transform)
                        .with_style(style.to_object_style(&transform));
                    object.name = "Scanline fill".to_string();
                    doc.add_object(object);
                }
                return;
            }
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
    warnings: &mut Vec<String>,
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
    let bundled = font.is_empty()
        || font.eq_ignore_ascii_case("sans")
        || font.starts_with(crate::textfont::BUNDLED_NAME);
    if !bundled && crate::textfont::resolve(&font).is_none() {
        let sample: String = content.chars().take(40).collect();
        warnings.push(format!(
            "font '{font}' is not installed; text '{sample}' uses the bundled font"
        ));
    }
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
/// Gradients use the colour at offset 0.5 and patterns their most frequent
/// line stroke or fill (equal counts are averaged). The approximation is reported once; a missing reference is
/// reported by id. Without this pass the shapes would silently lose their
/// paint.
fn resolve_gradient_paints(root: &mut Node, warnings: &mut Vec<String>) {
    let mut by_id: HashMap<String, &Node> = HashMap::new();
    collect_ids(root, &mut by_id);
    let mut colors: HashMap<String, String> = HashMap::new();
    let mut pattern_ids: HashSet<String> = HashSet::new();
    let mut gradient_ids: HashSet<String> = HashSet::new();
    for (id, node) in &by_id {
        let color = match node.name.as_str() {
            "linearGradient" | "radialGradient" => {
                gradient_ids.insert(id.clone());
                gradient_color(node, &by_id, 0)
            }
            "pattern" => {
                pattern_ids.insert(id.clone());
                pattern_color(node, 0)
            }
            _ => None,
        };
        if let Some(color) = color {
            colors.insert(id.clone(), color);
        }
    }
    let mut approximate = false;
    let mut missing: Vec<String> = Vec::new();
    rewrite_paints(
        root,
        &colors,
        &pattern_ids,
        &gradient_ids,
        &mut approximate,
        &mut missing,
    );
    if approximate {
        warnings.push(
            "gradients (bands or midpoint colour) and patterns (expanded or dominant paint) are approximated; the source is not reproduced"
                .to_string(),
        );
    }
    for id in missing {
        warnings.push(format!("paint reference #{id} was not found"));
    }
}

fn gradient_stops(node: &Node, by_id: &HashMap<String, &Node>, depth: usize) -> Vec<(f64, String)> {
    if depth > 8 {
        return Vec::new();
    }
    let mut stops: Vec<(f64, String)> = node
        .children
        .iter()
        .filter(|child| child.name == "stop")
        .filter_map(|stop| {
            let offset = gradient_offset(stop);
            let color = stop_color(stop)?;
            let color = crate::color::normalize_hex_color(&color)?;
            Some((offset, color))
        })
        .collect();
    if stops.is_empty() {
        let Some(href) = node.attr("href").or_else(|| node.attr("xlink:href")) else {
            return stops;
        };
        let Some(id) = href.trim().strip_prefix('#') else {
            return stops;
        };
        let Some(target) = by_id.get(id) else {
            return stops;
        };
        return gradient_stops(target, by_id, depth + 1);
    }
    stops.sort_by(|left, right| left.0.total_cmp(&right.0));
    stops
}

fn gradient_color(node: &Node, by_id: &HashMap<String, &Node>, depth: usize) -> Option<String> {
    let stops = gradient_stops(node, by_id, depth);
    color_at(&stops, 0.5)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn color_at(stops: &[(f64, String)], target: f64) -> Option<String> {
    let first = stops.first()?;
    if stops.len() == 1 {
        return Some(first.1.clone());
    }
    let mut left = &stops[0];
    let mut right = &stops[stops.len() - 1];
    if target > stops[0].0 && target < stops[stops.len() - 1].0 {
        for pair in stops.windows(2) {
            if target >= pair[0].0 && target <= pair[1].0 {
                left = &pair[0];
                right = &pair[1];
                break;
            }
        }
    }
    let (red_a, green_a, blue_a) = crate::color::parse_hex_color(&left.1)?;
    let (red_b, green_b, blue_b) = crate::color::parse_hex_color(&right.1)?;
    let blend = if (right.0 - left.0).abs() < 1e-12 {
        0.0
    } else {
        ((target - left.0) / (right.0 - left.0)).clamp(0.0, 1.0)
    };
    let mix = |start: u8, end: u8| {
        (f64::from(start) + (f64::from(end) - f64::from(start)) * blend)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Some(crate::color::to_hex(
        mix(red_a, red_b),
        mix(green_a, green_b),
        mix(blue_a, blue_b),
    ))
}

fn gradient_offset(stop: &Node) -> f64 {
    let Some(value) = stop.attr("offset") else {
        return 0.0;
    };
    let value = value.trim();
    let number = if let Some(percent) = value.strip_suffix('%') {
        percent.trim().parse::<f64>().map(|value| value / 100.0)
    } else {
        value.parse::<f64>()
    };
    number.unwrap_or(0.0).clamp(0.0, 1.0)
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
    let mut paints: HashMap<String, (usize, bool)> = HashMap::new();
    collect_pattern_paints(node, depth, &mut paints);
    let maximum = paints.values().map(|(count, _)| *count).max()?;
    let lines: Vec<&String> = paints
        .iter()
        .filter(|(_, (count, line))| *count == maximum && *line)
        .map(|(color, _)| color)
        .collect();
    if !lines.is_empty() {
        return average_colors(&lines);
    }
    let fills: Vec<&String> = paints
        .iter()
        .filter(|(_, (count, _))| *count == maximum)
        .map(|(color, _)| color)
        .collect();
    average_colors(&fills)
}

fn collect_pattern_paints(node: &Node, depth: usize, paints: &mut HashMap<String, (usize, bool)>) {
    if depth > 8 {
        return;
    }
    for child in &node.children {
        let fill = paint_declaration(child, "fill");
        let stroke = paint_declaration(child, "stroke");
        match (fill, stroke) {
            (Some(Some(fill)), _) => {
                let entry = paints.entry(fill).or_insert((0, false));
                entry.0 += 1;
            }
            (_, Some(Some(stroke))) => {
                let entry = paints.entry(stroke).or_insert((0, false));
                entry.0 += 1;
                entry.1 = true;
            }
            _ => {}
        }
        collect_pattern_paints(child, depth + 1, paints);
    }
}

#[allow(clippy::option_option)]
fn paint_declaration(node: &Node, name: &str) -> Option<Option<String>> {
    if let Some(value) = node.attr(name) {
        return paint(value);
    }
    let style = node.attr("style")?;
    for declaration in style.split(';') {
        let Some((key, value)) = declaration.split_once(':') else {
            continue;
        };
        if key.trim() == name {
            return paint(value);
        }
    }
    None
}

fn average_colors(colors: &[&String]) -> Option<String> {
    let first = colors.first()?;
    let mut sums = (0u64, 0u64, 0u64);
    let mut count = 0u64;
    for color in colors {
        if let Some((red, green, blue)) = crate::color::parse_hex_color(color) {
            sums.0 += u64::from(red);
            sums.1 += u64::from(green);
            sums.2 += u64::from(blue);
            count += 1;
        }
    }
    if count == 0 {
        return Some((*first).clone());
    }
    let channel = |value: u64| u8::try_from(value / count).unwrap_or(255);
    Some(crate::color::to_hex(
        channel(sums.0),
        channel(sums.1),
        channel(sums.2),
    ))
}

fn is_pattern_shape(node: &Node) -> bool {
    matches!(
        node.name.as_str(),
        "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "path"
    )
}

fn rewrite_paints(
    node: &mut Node,
    colors: &HashMap<String, String>,
    pattern_ids: &HashSet<String>,
    gradient_ids: &HashSet<String>,
    approximate: &mut bool,
    missing: &mut Vec<String>,
) {
    for attribute in ["fill", "stroke"] {
        if let Some(value) = node.attrs.get(attribute).cloned() {
            if let Some((id, resolved)) = resolve_paint_url(&value, colors) {
                if attribute == "fill" && is_pattern_shape(node) && pattern_ids.contains(&id) {
                    node.attrs.insert("data-pattern-fill".to_string(), id);
                } else if attribute == "fill"
                    && is_pattern_shape(node)
                    && gradient_ids.contains(&id)
                {
                    node.attrs.insert("data-gradient-fill".to_string(), id);
                } else {
                    node.attrs.insert(attribute.to_string(), resolved);
                }
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
        rewrite_paints(
            child,
            colors,
            pattern_ids,
            gradient_ids,
            approximate,
            missing,
        );
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

    fn total_filled_area(objects: &[Object]) -> f64 {
        objects
            .iter()
            .map(|object| {
                let points: Vec<(f64, f64)> = object
                    .world_path()
                    .flatten(0.1)
                    .into_iter()
                    .map(|point| (point.x, point.y))
                    .collect();
                crate::geometry::boolean::polygon_area(&points).abs()
            })
            .sum()
    }

    #[test]
    fn three_mixed_winding_contours_are_resolved() {
        let report = import_svg_report(
            r#"<svg width="20mm" height="20mm" viewBox="0 0 20 20"><path fill="black" d="M1 1 H9 V9 H1 Z M5 5 V13 H13 V5 Z M8 8 H16 V16 H8 Z"/></svg>"#,
        )
        .unwrap();
        assert!(!report.document.objects.is_empty(), "{:?}", report.warnings);
        assert!(
            report
                .document
                .objects
                .iter()
                .all(|object| matches!(object.shape, Shape::Path(_))),
            "{:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("scanline")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn three_even_odd_overlaps_are_resolved_by_scanline() {
        let report = import_svg_report(
            r#"<svg width="120mm" height="100mm" viewBox="0 0 120 100"><path fill="black" fill-rule="evenodd" d="M10 10 H50 V50 H10 Z M30 10 H70 V50 H30 Z M50 10 H90 V50 H50 Z"/></svg>"#,
        )
        .unwrap();
        let area = total_filled_area(&report.document.objects);
        assert!(
            (area - 1600.0).abs() < 1e-6,
            "area={area} {:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("scanline")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn three_mixed_windings_are_resolved_by_scanline() {
        let report = import_svg_report(
            r#"<svg width="120mm" height="100mm" viewBox="0 0 120 100"><path fill="black" d="M10 10 H50 V50 H10 Z M30 10 V50 H70 V10 Z M50 10 H90 V50 H50 Z"/></svg>"#,
        )
        .unwrap();
        let area = total_filled_area(&report.document.objects);
        assert!(
            (area - 1600.0).abs() < 1e-6,
            "area={area} {:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("scanline")),
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
    fn a_missing_font_falls_back_to_the_bundled_face() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm"><text x="1" y="5" font-family="Definitely Missing" font-size="4">A</text></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("Definitely Missing") && warning.contains("bundled font")
            }),
            "{:?}",
            report.warnings
        );
        let crate::document::Shape::Text(text) = &report.document.objects[0].shape else {
            panic!("expected text");
        };
        assert_eq!(text.font, "Definitely Missing");
        let paths = crate::objects::text_to_paths(text).unwrap();
        assert!(
            !paths.is_empty(),
            "the bundled fallback produced no outlines"
        );
    }

    #[test]
    fn the_bundled_font_is_not_reported_as_missing() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm"><text x="1" y="5" font-family="sans" font-size="4">A</text></svg>"#,
        )
        .unwrap();
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("font '")),
            "{:?}",
            report.warnings
        );
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
    fn a_use_clip_moves_with_the_reference() {
        let report = import_svg_report(
            r##"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><rect id="r" width="4" height="4"/><clipPath id="c"><rect width="5" height="5"/></clipPath></defs><use href="#r" x="2" y="2" clip-path="url(#c)"/></svg>"##,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 2.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 6.0).abs() < 1e-6, "{max:?}");
        assert!((min.y - 4.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 8.0).abs() < 1e-6, "{max:?}");
    }

    #[test]
    fn a_clip_path_trims_an_open_path() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><clipPath id="c"><rect x="2" y="2" width="6" height="6"/></clipPath></defs><line x1="0" y1="5" x2="10" y2="5" clip-path="url(#c)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 2.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 8.0).abs() < 1e-6, "{max:?}");
        let Shape::Path(path) = &report.document.objects[0].shape else {
            panic!("the clipped line is not a path");
        };
        assert!(!path.closed);
    }

    #[test]
    fn a_mask_is_approximated_as_a_clip_path() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><mask id="m"><rect x="1" y="1" width="4" height="4" fill="white"/></mask></defs><rect x="0" y="0" width="10" height="10" mask="url(#m)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 5.0).abs() < 1e-6, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("approximated as a clip path")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_group_clip_applies_to_every_child() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><clipPath id="c"><rect x="1" y="1" width="4" height="4"/></clipPath></defs><g clip-path="url(#c)"><rect x="0" y="0" width="10" height="10"/><rect x="20" y="20" width="5" height="5"/></g></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 5.0).abs() < 1e-6, "{max:?}");
        assert!((min.y - 5.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 9.0).abs() < 1e-6, "{max:?}");
    }

    #[test]
    fn a_clip_path_outlines_text() {
        let report = import_svg_report(
            r#"<svg width="100mm" height="100mm"><defs><clipPath id="c"><rect x="0" y="0" width="15" height="100"/></clipPath></defs><text x="10" y="80" font-size="20" clip-path="url(#c)">A</text></svg>"#,
        )
        .unwrap();
        assert!(!report.document.objects.is_empty(), "{:?}", report.warnings);
        assert!(
            report
                .document
                .objects
                .iter()
                .all(|object| matches!(object.shape, Shape::Path(_))),
            "{:?}",
            report.warnings
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("converting it to outlines")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_clip_path_masks_an_image() {
        let report = import_svg_report(
            r#"<svg width="100mm" height="100mm"><defs><clipPath id="c"><rect x="0" y="0" width="50" height="100"/></clipPath></defs><image href="test.png" x="10" y="80" width="80" height="80" clip-path="url(#c)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let Shape::Image(image) = &report.document.objects[0].shape else {
            panic!("the clipped image is not an image");
        };
        assert_eq!(image.mask_paths.len(), 1, "{:?}", report.warnings);
        assert!(!image.mask_invert);
        let (min, max) = image.mask_paths[0].bounding_box().unwrap();
        assert!((min.x + 2.6458).abs() < 1e-3, "{min:?}");
        assert!((max.x - 10.5833).abs() < 1e-3, "{max:?}");
        assert!((max.y - 21.1667).abs() < 1e-3, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("as a vector mask")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_filter_is_reported_and_left_unapplied() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><filter id="f"><feGaussianBlur stdDeviation="1"/><feOffset dx="2" dy="2"/></filter></defs><rect x="1" y="1" width="4" height="4" filter="url(#f)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 5.0).abs() < 1e-6, "{max:?}");
        assert!(
            report.warnings.iter().any(|warning| {
                warning.contains("filter #f")
                    && warning.contains("feGaussianBlur")
                    && warning.contains("feOffset")
                    && warning.contains("imported unfiltered")
            }),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn an_offset_filter_translates_the_shape() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><filter id="f"><feOffset dx="2" dy="1"/></filter></defs><rect x="1" y="1" width="4" height="4" filter="url(#f)"/></svg>"#,
        )
        .unwrap();
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 3.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 7.0).abs() < 1e-6, "{max:?}");
        assert!((min.y - 4.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 8.0).abs() < 1e-6, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("applied as a translation")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_saturate_filter_desaturates_the_fill() {
        let report = import_svg_report(
            r##"<svg width="10mm" height="10mm"><defs><filter id="f"><feColorMatrix type="saturate" values="0"/></filter></defs><rect width="5" height="5" fill="#ff0000" filter="url(#f)"/></svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#363636")
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("saturate")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_group_offset_filter_moves_every_child() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><filter id="f"><feOffset dx="1" dy="0"/></filter></defs><g filter="url(#f)"><rect x="0" y="0" width="2" height="2"/><rect x="5" y="0" width="2" height="2"/></g></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 2);
        let (min, _) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        let (min, _) = report.document.objects[1].bounds().unwrap();
        assert!((min.x - 6.0).abs() < 1e-6, "{min:?}");
    }

    #[test]
    fn a_filter_none_is_not_reported() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm"><rect width="5" height="5" filter="none"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1);
        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.contains("filter")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_missing_filter_is_reported() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm"><rect width="5" height="5" filter="url(#missing)"/></svg>"#,
        )
        .unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("missing #missing")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_group_filter_is_reported_once() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><filter id="f"><feBlend mode="multiply"/></filter></defs><g filter="url(#f)"><rect width="4" height="4"/></g></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("filter #f (feBlend)") && warning.contains("<g>")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_clip_path_intersects_the_shape() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><clipPath id="c"><rect x="1" y="1" width="4" height="4"/></clipPath></defs><rect x="0" y="0" width="10" height="10" clip-path="url(#c)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 1.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 5.0).abs() < 1e-6, "{max:?}");
        assert!((min.y - 5.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 9.0).abs() < 1e-6, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("applied as straight segments")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_missing_clip_path_is_reported() {
        let report = import_svg_report(
            r#"<svg width="10mm" height="10mm"><rect width="5" height="5" clip-path="url(#missing)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("missing #missing")),
            "{:?}",
            report.warnings
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
    fn an_external_use_clip_can_live_in_the_external_file() {
        let directory =
            std::env::temp_dir().join(format!("bw_svg_ext_file_clip_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("external.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><rect id="r" width="4" height="4"/><clipPath id="c"><rect width="3" height="3"/></clipPath></defs></svg>"#,
        )
        .unwrap();
        let main = directory.join("main.svg");
        std::fs::write(
            &main,
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><use href="external.svg#r" x="2" y="2" clip-path="url(#c)"/></svg>"#,
        )
        .unwrap();
        let report = import_svg_file_report(&main, &|| false).unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 2.0).abs() < 1e-3, "{min:?}");
        assert!((max.x - 5.0).abs() < 1e-3, "{max:?}");
        assert!((min.y - 5.0).abs() < 1e-3, "{min:?}");
        assert!((max.y - 8.0).abs() < 1e-3, "{max:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("clip path #c was applied")),
            "{:?}",
            report.warnings
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_external_use_keeps_its_clip() {
        let directory =
            std::env::temp_dir().join(format!("bw_svg_ext_clip_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("external.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs><rect id="r" width="4" height="4"/></defs></svg>"#,
        )
        .unwrap();
        let main = directory.join("main.svg");
        std::fs::write(
            &main,
            r#"<svg width="10mm" height="10mm" viewBox="0 0 10 10"><defs><clipPath id="c"><rect width="5" height="5"/></clipPath></defs><use href="external.svg#r" x="2" y="2" clip-path="url(#c)"/></svg>"#,
        )
        .unwrap();
        let report = import_svg_file_report(&main, &|| false).unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        let (min, max) = report.document.objects[0].bounds().unwrap();
        assert!((min.x - 2.0).abs() < 1e-6, "{min:?}");
        assert!((max.x - 6.0).abs() < 1e-6, "{max:?}");
        assert!((min.y - 4.0).abs() < 1e-6, "{min:?}");
        assert!((max.y - 8.0).abs() < 1e-6, "{max:?}");
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
    fn a_large_linear_gradient_uses_more_bands() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="200mm" height="20mm" viewBox="0 0 200 20">
                <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="200" y2="0">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </linearGradient></defs>
                <rect x="0" y="0" width="200" height="20" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("64 band(s)")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_small_linear_gradient_keeps_sixteen_bands() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="10" y2="0">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </linearGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("16 band(s)")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_radial_gradient_is_expanded_into_bands() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="5" cy="5" r="4">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </radialGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 2, "{:?}", report.warnings);
        assert!(
            report
                .document
                .objects
                .iter()
                .any(|object| object.hole_of.is_some()),
            "the rings have no holes"
        );
        let fills: std::collections::HashSet<_> = report
            .document
            .objects
            .iter()
            .filter_map(|object| object.style.fill.clone())
            .collect();
        assert!(fills.len() > 1, "the radial gradient kept one colour");
        for object in &report.document.objects {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_radial_gradient_uses_its_midpoint() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><radialGradient id="g" r="2">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </radialGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#800080")
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
    fn a_three_stop_radial_gradient_uses_the_middle_stop() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><radialGradient id="g" r="2">
                    <stop offset="0%" stop-color="#ff0000"/>
                    <stop offset="50%" stop-color="#00ff00"/>
                    <stop offset="100%" stop-color="#0000ff"/>
                </radialGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#00ff00")
        );
    }

    #[test]
    fn a_linear_gradient_is_expanded_into_bands() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><linearGradient id="g">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff"/>
                </linearGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        let fills: std::collections::HashSet<_> = report.document.objects[1..]
            .iter()
            .filter_map(|object| object.style.fill.clone())
            .collect();
        assert!(fills.len() > 1, "the gradient produced a single colour");
        for object in &report.document.objects[1..] {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_user_space_linear_gradient_is_expanded_into_bands() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="10" y2="0">
                    <stop offset="0" stop-color="#ffffff"/>
                    <stop offset="1" stop-color="#000000"/>
                </linearGradient></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    fn has_diagonal_edge(object: &Object) -> bool {
        object.world_path().segments.iter().any(|segment| {
            if let Segment::Line { from, to } = segment {
                let dx = to.x - from.x;
                let dy = to.y - from.y;
                if dx.hypot(dy) < 1e-3 {
                    return false;
                }
                let angle = dy.atan2(dx).abs();
                (angle - std::f64::consts::FRAC_PI_4).abs() < 0.2
                    || (angle - 3.0 * std::f64::consts::FRAC_PI_4).abs() < 0.2
            } else {
                false
            }
        })
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
    fn a_pattern_prefers_its_line_colour() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><pattern id="p" patternUnits="objectBoundingBox">
                    <rect width="2" height="2" fill="#ffffff"/>
                    <path d="M0 0 L2 2" fill="none" stroke="#ff0000"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#ff0000")
        );
    }

    #[test]
    fn a_pattern_uses_its_most_frequent_paint() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><pattern id="p" patternUnits="objectBoundingBox">
                    <rect width="2" height="2" fill="#ffffff"/>
                    <path d="M0 0 L2 2" fill="none" stroke="#ff0000"/>
                    <path d="M0 2 L2 0" fill="none" stroke="#ff0000"/>
                    <path d="M0 1 L2 1" fill="none" stroke="#0000ff"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#ff0000")
        );
    }

    #[test]
    fn a_pattern_averages_tied_fills() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm">
                <defs><pattern id="p" patternUnits="objectBoundingBox">
                    <rect width="1" height="1" fill="#00ff00"/>
                    <rect width="1" height="1" fill="#0000ff"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#007f7f")
        );
    }

    #[test]
    fn a_user_space_pattern_is_expanded_into_tiles() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse">
                    <path d="M0 0 L2 2" fill="none" stroke="#ff0000"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        assert!(
            report.document.objects[0].style.fill.is_none(),
            "the pattern fill must not also be a solid fill"
        );
        for object in &report.document.objects[1..] {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn a_filled_pattern_tile_is_clipped_to_the_shape() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse">
                    <rect width="1" height="1" fill="#00ff00"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        for object in &report.document.objects[1..] {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn an_object_bounding_box_pattern_is_expanded_into_tiles() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><pattern id="p" width="0.5" height="0.5" patternUnits="objectBoundingBox">
                    <rect width="0.25" height="0.25" fill="#00ff00"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        for object in &report.document.objects[1..] {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("expanded into")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn an_object_bounding_box_pattern_follows_a_rotated_shape() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">
                <defs><pattern id="p" width="0.5" height="0.5" patternUnits="objectBoundingBox">
                    <rect width="0.25" height="0.25" fill="#00ff00"/>
                </pattern></defs>
                <rect x="5" y="5" width="10" height="10" transform="rotate(45 10 10)" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        let centre = Point2::new(10.0, 10.0);
        let closest = report.document.objects[1..]
            .iter()
            .min_by(|left, right| {
                let distance = |object: &Object| {
                    let (min, max) = object.bounds().unwrap();
                    let middle =
                        Point2::new(f64::midpoint(min.x, max.x), f64::midpoint(min.y, max.y));
                    middle.distance(centre)
                };
                distance(left).total_cmp(&distance(right))
            })
            .unwrap();
        assert!(
            has_diagonal_edge(closest),
            "the centre tile kept the world-aligned bbox"
        );
    }

    #[test]
    fn a_pattern_transform_rotates_the_tiles() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                    <path d="M0 1 L2 1" fill="none" stroke="#ff0000"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert!(report.document.objects.len() > 1, "{:?}", report.warnings);
        assert!(
            report.document.objects[1..].iter().any(has_diagonal_edge),
            "the pattern transform was not applied to the tile lattice"
        );
        for object in &report.document.objects[1..] {
            let (min, max) = object.bounds().unwrap();
            assert!(min.x >= -1e-6 && max.x <= 10.0 + 1e-6, "{min:?} {max:?}");
            assert!(min.y >= -1e-6 && max.y <= 10.0 + 1e-6, "{min:?} {max:?}");
        }
    }

    #[test]
    fn a_pattern_without_a_tile_size_falls_back_to_a_colour() {
        let report = import_svg_report(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
                <defs><pattern id="p" patternUnits="objectBoundingBox">
                    <rect width="1" height="1" fill="#00ff00"/>
                    <rect width="1" height="1" fill="#0000ff"/>
                </pattern></defs>
                <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(report.document.objects.len(), 1, "{:?}", report.warnings);
        assert_eq!(
            report.document.objects[0].style.fill.as_deref(),
            Some("#007f7f")
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("tile size")),
            "{:?}",
            report.warnings
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
