use serde::{Deserialize, Serialize};

use crate::geometry::Point2;

/// Horizontal alignment of a text block around its anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Center => "Centre",
            TextAlign::Right => "Right",
        }
    }
}

/// Vertical alignment of a text block around its anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextVAlign {
    /// The anchor is the bottom of the block.
    #[default]
    Bottom,
    /// The anchor is the vertical centre of the block.
    Middle,
    /// The anchor is the top of the block.
    Top,
}

impl TextVAlign {
    pub fn label(self) -> &'static str {
        match self {
            TextVAlign::Bottom => "Bottom",
            TextVAlign::Middle => "Middle",
            TextVAlign::Top => "Top",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextObject {
    pub content: String,
    pub font: String,
    pub size: f64,
    /// Anchor point: bottom-left, bottom-centre or bottom-right according to
    /// `align`, in local coordinates.
    pub x: f64,
    pub y: f64,
    /// Extra space between characters, in millimetres.
    #[serde(default)]
    pub letter_spacing: f64,
    /// Line height as a multiple of the font size.
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f64,
    #[serde(default)]
    pub align: TextAlign,
    /// Draw the content in upper case without changing what is stored.
    #[serde(default)]
    pub uppercase: bool,
    /// Vertical placement of the block around the anchor.
    #[serde(default)]
    pub vertical_align: TextVAlign,
    /// Use the bold face of the chosen font when one is available.
    #[serde(default)]
    pub bold: bool,
    /// Use the italic/oblique face of the chosen font when one is available.
    #[serde(default)]
    pub italic: bool,
}

fn default_line_spacing() -> f64 {
    1.2
}

impl Default for TextObject {
    fn default() -> Self {
        Self {
            content: "Text".to_string(),
            font: "sans".to_string(),
            size: 10.0,
            x: 0.0,
            y: 0.0,
            letter_spacing: 0.0,
            line_spacing: default_line_spacing(),
            align: TextAlign::Left,
            uppercase: false,
            vertical_align: TextVAlign::Bottom,
            bold: false,
            italic: false,
        }
    }
}

impl TextObject {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.content.is_empty() {
            return Err("text content must not be empty".to_string());
        }
        if !self.size.is_finite() || self.size <= 0.0 {
            return Err("font size must be positive".to_string());
        }
        if !self.letter_spacing.is_finite() {
            return Err("letter spacing must be finite".to_string());
        }
        if !self.line_spacing.is_finite() || !(0.5..=10.0).contains(&self.line_spacing) {
            return Err("line spacing must be between 0.5 and 10".to_string());
        }
        Ok(())
    }

    pub fn lines(&self) -> Vec<&str> {
        self.content.split('\n').collect()
    }

    /// Lines as they are drawn, applying the upper-case switch.
    pub fn rendered_lines(&self) -> Vec<String> {
        let content = self.rendered();
        content.split('\n').map(str::to_string).collect()
    }

    /// The content as it is drawn, applying the upper-case switch.
    pub fn rendered(&self) -> String {
        if self.uppercase {
            self.content.to_uppercase()
        } else {
            self.content.clone()
        }
    }

    /// Height of one line, including the extra line spacing.
    pub fn line_height(&self) -> f64 {
        self.size * self.line_spacing
    }

    /// Width of one line, approximated from the font size and character count.
    #[allow(clippy::cast_precision_loss)]
    pub fn line_width(&self, line: &str) -> f64 {
        let characters = line.chars().count() as f64;
        if characters == 0.0 {
            return 0.0;
        }
        characters * self.size * 0.5 + (characters - 1.0) * self.letter_spacing
    }

    /// Width of the widest line.
    pub fn block_width(&self) -> f64 {
        self.rendered_lines()
            .iter()
            .map(|line| self.line_width(line))
            .fold(0.0, f64::max)
    }

    /// Height of the whole block.
    pub fn block_height(&self) -> f64 {
        let lines = self.rendered_lines().len();
        if lines == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let extra = (lines - 1) as f64 * self.line_height();
        self.size + extra
    }

    /// Left edge of the block after applying the alignment.
    pub fn left(&self) -> f64 {
        let width = self.block_width();
        match self.align {
            TextAlign::Left => self.x,
            TextAlign::Center => self.x - width / 2.0,
            TextAlign::Right => self.x - width,
        }
    }

    /// Approximate width of the widest line (kept for callers of the old name).
    pub fn approx_width(&self) -> f64 {
        self.block_width()
    }

    /// Bottom of the block after applying the vertical alignment.
    pub fn block_bottom(&self) -> f64 {
        let height = self.block_height();
        match self.vertical_align {
            TextVAlign::Bottom => self.y,
            TextVAlign::Middle => self.y - height / 2.0,
            TextVAlign::Top => self.y - height,
        }
    }

    /// Approximate bounding box in local coordinates.
    pub fn bounds(&self) -> (Point2, Point2) {
        let left = self.left();
        let bottom = self.block_bottom();
        (
            Point2::new(left, bottom),
            Point2::new(left + self.block_width(), bottom + self.block_height()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{TextAlign, TextObject, TextVAlign};

    fn sample() -> TextObject {
        TextObject {
            content: "AB".to_string(),
            font: "sans".to_string(),
            size: 10.0,
            x: 2.0,
            y: 3.0,
            ..TextObject::default()
        }
    }

    #[test]
    fn valid() {
        assert!(sample().validate().is_ok());
    }

    #[test]
    fn uppercase_is_a_drawing_switch_not_a_content_edit() {
        let mut text = TextObject {
            content: "ab\ncd".to_string(),
            ..TextObject::default()
        };
        assert_eq!(text.rendered_lines(), vec!["ab", "cd"]);
        text.uppercase = true;
        assert_eq!(text.rendered(), "AB\nCD");
        assert_eq!(text.rendered_lines(), vec!["AB", "CD"]);
        assert_eq!(text.content, "ab\ncd");
    }

    #[test]
    fn reject_empty_content() {
        let text = TextObject {
            content: String::new(),
            ..sample()
        };
        assert!(text.validate().is_err());
    }

    #[test]
    fn reject_bad_size() {
        let text = TextObject {
            size: 0.0,
            ..sample()
        };
        assert!(text.validate().is_err());
        let text = TextObject {
            size: f64::NAN,
            ..sample()
        };
        assert!(text.validate().is_err());
    }

    #[test]
    fn reject_bad_spacing() {
        let letter = TextObject {
            letter_spacing: f64::NAN,
            ..sample()
        };
        assert!(letter.validate().is_err());
        let line = TextObject {
            line_spacing: 0.1,
            ..sample()
        };
        assert!(line.validate().is_err());
        let line = TextObject {
            line_spacing: 50.0,
            ..sample()
        };
        assert!(line.validate().is_err());
    }

    #[test]
    fn bounds() {
        let (min, max) = sample().bounds();
        assert!((min.x - 2.0).abs() < 1e-9);
        assert!((min.y - 3.0).abs() < 1e-9);
        assert!(max.x > min.x);
        assert!((max.y - 13.0).abs() < 1e-9);
    }

    #[test]
    fn empty_text_has_zero_width() {
        let text = TextObject {
            content: String::new(),
            ..sample()
        };
        assert!((text.approx_width()).abs() < 1e-9);
        assert_eq!(text.block_height(), 10.0);
    }

    #[test]
    fn letter_spacing_widens_the_line() {
        let normal = sample();
        let spaced = TextObject {
            letter_spacing: 2.0,
            ..sample()
        };
        assert!((spaced.block_width() - normal.block_width() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn line_spacing_and_alignment_shape_the_block() {
        let block = TextObject {
            content: "A\nBB".to_string(),
            line_spacing: 2.0,
            ..sample()
        };
        // Two 10 mm lines with one 20 mm step between their baselines.
        assert!((block.block_height() - 30.0).abs() < 1e-9);
        let left = block.bounds();
        let centered = TextObject {
            align: TextAlign::Center,
            ..block.clone()
        };
        let right = TextObject {
            align: TextAlign::Right,
            ..block.clone()
        };
        assert!((centered.bounds().0.x - (left.0.x - block.block_width() / 2.0)).abs() < 1e-9);
        assert!((right.bounds().0.x - (left.0.x - block.block_width())).abs() < 1e-9);
        assert!((right.bounds().1.x - block.x).abs() < 1e-9);
    }

    #[test]
    fn vertical_alignment_moves_the_block_around_the_anchor() {
        let mut text = TextObject {
            content: "A\nB".to_string(),
            size: 10.0,
            line_spacing: 2.0,
            y: 50.0,
            ..TextObject::default()
        };
        let height = text.block_height();
        assert!((text.block_bottom() - 50.0).abs() < 1e-9);
        text.vertical_align = TextVAlign::Middle;
        assert!((text.block_bottom() - (50.0 - height / 2.0)).abs() < 1e-9);
        text.vertical_align = TextVAlign::Top;
        assert!((text.block_bottom() - (50.0 - height)).abs() < 1e-9);
        let (min, max) = text.bounds();
        assert!((max.y - 50.0).abs() < 1e-9);
        assert!((min.y - (50.0 - height)).abs() < 1e-9);
    }

    #[test]
    fn old_projects_without_metadata_still_load() {
        let old = r#"{"content":"Hi","font":"sans","size":5.0,"x":0.0,"y":0.0}"#;
        let text: TextObject = serde_json::from_str(old).unwrap();
        assert_eq!(text.align, TextAlign::Left);
        assert_eq!(text.letter_spacing, 0.0);
        assert!((text.line_spacing - 1.2).abs() < 1e-9);
        assert_eq!(text.vertical_align, TextVAlign::Bottom);
        assert!(!text.bold);
        assert!(!text.italic);
    }
}
