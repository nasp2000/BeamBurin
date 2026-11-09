use crate::document::Document;
use crate::document::model::{Object, Shape};
use crate::errors::{BeamError, Result};
use crate::objects::{ImageMode, ImageObject};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Default resolution assumed when the file carries no usable DPI metadata.
pub const DEFAULT_DPI: f64 = 96.0;

/// Import a raster image with the default pixel limit.
pub fn import_raster(path: &Path) -> Result<Document> {
    import_raster_with_limit(path, crate::raster::MAX_PIXELS)
}

/// Import a raster image as one image object, sized at [`DEFAULT_DPI`].
///
/// `max_pixels` rejects images that would use more memory than the configured
/// limit before they are decoded into a document.
pub fn import_raster_with_limit(path: &Path, max_pixels: u64) -> Result<Document> {
    import_raster_with_limit_cancellable(path, max_pixels, &|| false)
}

/// Import a raster image, stopping inside the decoder when `cancel` is set.
///
/// The image library reads through [`CancellableReader`], so the flag is checked
/// on every file read; a cancelled import returns "cancelled" instead of
/// finishing the decode in the background.
pub fn import_raster_with_limit_cancellable(
    path: &Path,
    max_pixels: u64,
    cancel: &dyn Fn() -> bool,
) -> Result<Document> {
    if cancel() {
        return Err(BeamError::InvalidInput("cancelled".to_string()));
    }
    // Read the dimensions from the header first. Decoding a file that is over
    // the limit would allocate its full pixel buffer before the check below
    // could reject it.
    let (width_px, height_px) = crate::raster::image_dimensions(path)?;
    if width_px == 0 || height_px == 0 {
        return Err(BeamError::InvalidInput("image has no pixels".to_string()));
    }
    let pixels = u64::from(width_px) * u64::from(height_px);
    if pixels > max_pixels {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {max_pixels}"
        )));
    }
    let image = decode_cancellable(path, cancel)?;
    let width_px = image.width();
    let height_px = image.height();
    if width_px == 0 || height_px == 0 {
        return Err(BeamError::InvalidInput("image has no pixels".to_string()));
    }
    let pixels = u64::from(width_px) * u64::from(height_px);
    if pixels > max_pixels {
        return Err(BeamError::InvalidInput(format!(
            "image has {pixels} pixels; the limit is {max_pixels}"
        )));
    }
    let mut image_object =
        ImageObject::new(path.to_string_lossy(), width_px, height_px, DEFAULT_DPI);
    // Show the whole picture in grayscale first; the engraving mode and its
    // dithering are chosen afterwards in Adjust image.
    image_object.settings.mode = ImageMode::Grayscale;
    let mut document = Document {
        width_mm: image_object.width_mm(),
        height_mm: image_object.height_mm(),
        ..Document::default()
    };
    let layer_id = document.layers[0].id.clone();
    document
        .objects
        .push(Object::new(layer_id, Shape::Image(image_object)));
    Ok(document)
}

/// Decode through a reader that turns the cancel flag into an I/O error.
///
/// A plain `Interrupted` error would be retried by `read_exact`, so the reader
/// uses `Other` and the decode error is mapped back to "cancelled".
fn decode_cancellable(path: &Path, cancel: &dyn Fn() -> bool) -> Result<image::DynamicImage> {
    let file = File::open(path)?;
    let reader = CancellableReader {
        inner: BufReader::new(file),
        cancel,
    };
    image::ImageReader::new(reader)
        .with_guessed_format()
        .map_err(|error| BeamError::InvalidInput(format!("cannot read image: {error}")))?
        .decode()
        .map_err(|error| {
            if cancel() {
                BeamError::InvalidInput("cancelled".to_string())
            } else {
                BeamError::InvalidInput(format!("cannot read image: {error}"))
            }
        })
}

struct CancellableReader<'a, R> {
    inner: R,
    cancel: &'a dyn Fn() -> bool,
}

fn cancelled_read() -> io::Error {
    io::Error::other("cancelled")
}

impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if (self.cancel)() {
            return Err(cancelled_read());
        }
        self.inner.read(buffer)
    }
}

impl<R: BufRead> BufRead for CancellableReader<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if (self.cancel)() {
            return Err(cancelled_read());
        }
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

impl<R: Seek> Seek for CancellableReader<'_, R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        if (self.cancel)() {
            return Err(cancelled_read());
        }
        self.inner.seek(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn an_imported_photo_starts_in_grayscale() {
        let path = temp_dir().join("bw_import_grayscale.png");
        let image = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 120, 240, 255]));
        image.save(&path).unwrap();
        let document = import_raster(&path).unwrap();
        let Shape::Image(image) = &document.objects[0].shape else {
            panic!("the imported object is not an image");
        };
        assert_eq!(image.settings.mode, ImageMode::Grayscale);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cancel_stops_inside_the_decoder() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let path = temp_dir().join("bw_cancel_decode.png");
        let image = image::RgbaImage::from_fn(1024, 1024, |x, y| {
            let value = ((x * 7 + y * 13) % 251) as u8;
            image::Rgba([value, value.wrapping_mul(3), value.wrapping_add(17), 255])
        });
        image.save(&path).unwrap();
        let calls = AtomicUsize::new(0);
        let cancel = || calls.fetch_add(1, Ordering::Relaxed) >= 2;
        let error = import_raster_with_limit_cancellable(&path, crate::raster::MAX_PIXELS, &cancel)
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");
        assert!(
            calls.load(Ordering::Relaxed) > 2,
            "the decoder ignored the cancel flag"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_errors() {
        let path = temp_dir().join("bw_missing_image.png");
        let _ = std::fs::remove_file(&path);
        assert!(import_raster(&path).is_err());
    }

    #[test]
    fn a_pixel_limit_rejects_large_images() {
        let path = temp_dir().join("bw_limit_image.png");
        let image = image::RgbaImage::from_pixel(4, 3, image::Rgba([0, 0, 0, 255]));
        image.save(&path).unwrap();
        let error = import_raster_with_limit(&path, 10).unwrap_err();
        assert!(error.to_string().contains("limit"));
        assert_eq!(
            import_raster_with_limit(&path, 12).unwrap().objects.len(),
            1
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn imports_gif_and_webp() {
        let image = image::RgbaImage::from_pixel(4, 3, image::Rgba([0, 0, 0, 255]));
        let gif = temp_dir().join("bw_import_image.gif");
        image.save(&gif).unwrap();
        let document = import_raster(&gif).unwrap();
        assert_eq!(document.objects.len(), 1);
        let _ = std::fs::remove_file(&gif);

        let webp = temp_dir().join("bw_import_image.webp");
        image.save(&webp).unwrap();
        let document = import_raster(&webp).unwrap();
        assert_eq!(document.objects.len(), 1);
        let _ = std::fs::remove_file(&webp);
    }

    #[test]
    fn imports_png_with_physical_size() {
        let path = temp_dir().join("bw_import_image.png");
        let image = image::RgbaImage::from_pixel(4, 3, image::Rgba([255, 0, 0, 255]));
        image.save(&path).unwrap();
        let document = import_raster(&path).unwrap();
        assert!((document.width_mm - 4.0 * 25.4 / 96.0).abs() < 1e-9);
        assert!((document.height_mm - 3.0 * 25.4 / 96.0).abs() < 1e-9);
        assert_eq!(document.objects.len(), 1);
        match &document.objects[0].shape {
            Shape::Image(image) => {
                assert_eq!(image.width_px, 4);
                assert_eq!(image.dpi, DEFAULT_DPI);
                assert!(image.validate().is_ok());
            }
            other => panic!("unexpected shape: {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_huge_header_is_rejected_before_decoding() {
        // An IHDR-only PNG claiming 100000 x 100000 pixels. Decoding it would
        // fail because there is no image data; the header check must report the
        // pixel limit instead and never allocate the pixel buffer.
        let path = temp_dir().join("bw_huge_header.png");
        write_header_only_png(&path, 100_000, 100_000);
        let error = import_raster_with_limit(&path, crate::raster::MAX_PIXELS).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("10000000000 pixels"), "{message}");
        assert!(message.contains("limit is"), "{message}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_non_image_file() {
        let path = temp_dir().join("bw_not_an_image.png");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(import_raster(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    fn write_header_only_png(path: &Path, width: u32, height: u32) {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for byte in data {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    let mask = (crc & 1).wrapping_neg();
                    crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
                }
            }
            !crc
        }
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        png.extend_from_slice(&u32::try_from(ihdr.len()).unwrap().to_be_bytes());
        let mut chunk = b"IHDR".to_vec();
        chunk.extend_from_slice(&ihdr);
        png.extend_from_slice(&chunk);
        png.extend_from_slice(&crc32(&chunk).to_be_bytes());
        // An empty IDAT keeps the PNG header parseable while leaving no image
        // data to decode.
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&crc32(b"IDAT").to_be_bytes());
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&crc32(b"IEND").to_be_bytes());
        std::fs::write(path, png).unwrap();
    }
}
