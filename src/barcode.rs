//! Code 39 barcode geometry.
//!
//! The encoding is the public Code 39 table: each character is nine elements
//! (five bars and four spaces), three of them wide, with a narrow gap between
//! characters and the `*` start/stop character at both ends. Bars are returned
//! as millimetre rectangles so the caller can turn them into ordinary objects;
//! no scanner or machine code is involved.

/// One bar or space in the encoded symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarcodeElement {
    pub bar: bool,
    /// Width in modules: one for narrow, three for wide.
    pub modules: u8,
}

/// Code 39 patterns, one character each. `1` is a wide element, `0` narrow.
const CODE39: &[(char, &str)] = &[
    ('0', "000110100"),
    ('1', "100100001"),
    ('2', "001100001"),
    ('3', "101100000"),
    ('4', "000110001"),
    ('5', "100110000"),
    ('6', "001110000"),
    ('7', "000100101"),
    ('8', "100100100"),
    ('9', "001100100"),
    ('A', "100001001"),
    ('B', "001001001"),
    ('C', "101001000"),
    ('D', "000011001"),
    ('E', "100011000"),
    ('F', "001011000"),
    ('G', "000001101"),
    ('H', "100001100"),
    ('I', "001001100"),
    ('J', "000011100"),
    ('K', "100000011"),
    ('L', "001000011"),
    ('M', "101000010"),
    ('N', "000010011"),
    ('O', "100010010"),
    ('P', "001010010"),
    ('Q', "000000111"),
    ('R', "100000110"),
    ('S', "001000110"),
    ('T', "000010110"),
    ('U', "110000001"),
    ('V', "011000001"),
    ('W', "111000000"),
    ('X', "010010001"),
    ('Y', "110010000"),
    ('Z', "011010000"),
    ('-', "010000101"),
    ('.', "110000100"),
    (' ', "011000100"),
    ('$', "010101000"),
    ('/', "010100010"),
    ('+', "010001010"),
    ('%', "000101010"),
    ('*', "010010100"),
];

/// Longest text accepted, so a mistake cannot create thousands of objects.
pub const MAX_CHARACTERS: usize = 64;

fn pattern(character: char) -> Option<&'static str> {
    CODE39
        .iter()
        .find(|(candidate, _)| *candidate == character)
        .map(|(_, pattern)| *pattern)
}

/// Encode `text` as Code 39 bars and spaces, including the start/stop `*`.
pub fn encode(text: &str) -> Result<Vec<BarcodeElement>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("the barcode text is empty".to_string());
    }
    if text.chars().count() > MAX_CHARACTERS {
        return Err(format!(
            "the barcode text is limited to {MAX_CHARACTERS} characters"
        ));
    }
    let characters: Vec<char> = text.to_ascii_uppercase().chars().collect();
    let mut symbols = Vec::with_capacity(characters.len() + 2);
    symbols.push('*');
    symbols.extend(characters);
    symbols.push('*');
    let mut elements = Vec::new();
    for (index, symbol) in symbols.iter().enumerate() {
        if *symbol == '*' && index != 0 && index + 1 != symbols.len() {
            return Err("'*' is reserved for the start and stop character".to_string());
        }
        let pattern =
            pattern(*symbol).ok_or_else(|| format!("'{symbol}' is not a Code 39 character"))?;
        for (position, bit) in pattern.chars().enumerate() {
            elements.push(BarcodeElement {
                bar: position % 2 == 0,
                modules: if bit == '1' { 3 } else { 1 },
            });
        }
        if index + 1 < symbols.len() {
            elements.push(BarcodeElement {
                bar: false,
                modules: 1,
            });
        }
    }
    Ok(elements)
}

/// Bar rectangles `(x, width)` in millimetres, starting at x = 0.
pub fn bars(text: &str, module_mm: f64, height_mm: f64) -> Result<Vec<(f64, f64)>, String> {
    if !module_mm.is_finite() || module_mm <= 0.0 {
        return Err("the module width must be positive".to_string());
    }
    if !height_mm.is_finite() || height_mm <= 0.0 {
        return Err("the bar height must be positive".to_string());
    }
    let mut x = 0.0;
    let mut bars = Vec::new();
    for element in encode(text)? {
        let width = f64::from(element.modules) * module_mm;
        if element.bar {
            bars.push((x, width));
        }
        x += width;
    }
    Ok(bars)
}

/// Total width of the encoded symbol, in millimetres.
pub fn width_mm(text: &str, module_mm: f64) -> Result<f64, String> {
    let elements = encode(text)?;
    let modules: u32 = elements
        .iter()
        .map(|element| u32::from(element.modules))
        .sum();
    Ok(f64::from(modules) * module_mm)
}

#[cfg(test)]
mod tests {
    use super::{bars, encode, width_mm};

    #[test]
    fn a_character_starts_and_ends_with_the_stop_pattern() {
        let elements = encode("A").unwrap();
        assert_eq!(elements.len(), 9 + 1 + 9 + 1 + 9);
        let first: String = elements[..9]
            .iter()
            .map(|element| if element.modules == 3 { '1' } else { '0' })
            .collect();
        assert_eq!(first, "010010100");
        let last: String = elements[elements.len() - 9..]
            .iter()
            .map(|element| if element.modules == 3 { '1' } else { '0' })
            .collect();
        assert_eq!(last, "010010100");
        assert!(!elements[9].bar);
        assert_eq!(elements[9].modules, 1);
    }

    #[test]
    fn bars_are_millimetre_rectangles_with_narrow_and_wide_widths() {
        let bars = bars("A", 0.5, 10.0).unwrap();
        assert_eq!(bars.len(), 15);
        assert!(
            bars.iter()
                .all(|(_, width)| (*width - 0.5).abs() < 1e-9 || (*width - 1.5).abs() < 1e-9)
        );
        assert!((bars[0].0 - 0.0).abs() < 1e-9);
        // 47 modules for "*A*".
        assert!((width_mm("A", 0.5).unwrap() - 23.5).abs() < 1e-9);
    }

    #[test]
    fn lowercase_is_uppercased_and_bad_characters_are_refused() {
        assert_eq!(encode("ab").unwrap(), encode("AB").unwrap());
        assert!(encode("").is_err());
        assert!(encode("A@B").is_err());
        assert!(encode(&"A".repeat(65)).is_err());
        assert!(bars("A", 0.0, 10.0).is_err());
        assert!(bars("A", 0.5, 0.0).is_err());
    }
}
