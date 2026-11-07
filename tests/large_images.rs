//! Large-photo budget checks.
//!
//! This test is ignored in the normal suite because it builds a multi-megapixel
//! JPEG. The scheduled `corpus` workflow runs it with `--ignored`, which keeps
//! the pull-request lane fast while still guarding the import and first-preview
//! path against a return of the multi-second freeze on 4–5 MB photos.

use std::time::{Duration, Instant};

use beamburin::document::Shape;
use beamburin::raster::{
    MAX_PIXELS, PREVIEW_MAX_SIDE, auto_levels_preview, histogram_preview, process_object_preview,
};

#[test]
#[ignore = "large image budget; run by the scheduled corpus workflow"]
#[allow(clippy::cast_possible_truncation)]
fn a_multi_megabyte_photo_imports_and_previews_within_budget() {
    let path = std::env::temp_dir().join("bw_large_photo_budget.jpg");
    let _ = std::fs::remove_file(&path);

    let width = 4000;
    let height = 3000;
    let mut picture = image::RgbImage::new(width, height);
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for pixel in picture.pixels_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let red = (state >> 8) as u8;
        let green = (state >> 24) as u8;
        let blue = (state >> 40) as u8;
        *pixel = image::Rgb([red, green, blue]);
    }
    let file = std::fs::File::create(&path).expect("create large JPEG");
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 95);
    encoder.encode_image(&picture).expect("encode large JPEG");
    drop(picture);
    let bytes = std::fs::metadata(&path).expect("metadata").len();
    assert!(bytes >= 1_000_000, "the fixture is only {bytes} bytes");
    println!("large fixture: {width} x {height}, {bytes} bytes");

    let import_start = Instant::now();
    let document = beamburin::import::raster::import_raster_with_limit(&path, MAX_PIXELS)
        .expect("import large JPEG");
    let import_time = import_start.elapsed();
    let Shape::Image(image) = &document.objects[0].shape else {
        panic!("the imported object is not an image");
    };
    assert_eq!((image.width_px, image.height_px), (width, height));
    assert!(
        import_time < Duration::from_secs(30),
        "import took {import_time:?}"
    );
    println!("import: {import_time:?}");

    let preview_start = Instant::now();
    let preview = process_object_preview(&path, image, PREVIEW_MAX_SIDE).expect("preview");
    let preview_time = preview_start.elapsed();
    assert!(
        preview.width.max(preview.height) <= PREVIEW_MAX_SIDE as usize,
        "preview is {} x {}",
        preview.width,
        preview.height
    );
    assert!(
        preview_time < Duration::from_secs(20),
        "preview took {preview_time:?}"
    );
    println!(
        "preview: {} x {}, {preview_time:?}",
        preview.width, preview.height
    );

    let histogram_start = Instant::now();
    let bins = histogram_preview(&path, image, PREVIEW_MAX_SIDE).expect("histogram");
    let histogram_time = histogram_start.elapsed();
    assert_eq!(
        bins.iter().sum::<u64>() as usize,
        preview.width * preview.height
    );
    assert!(
        histogram_time < Duration::from_secs(20),
        "histogram took {histogram_time:?}"
    );
    let (black, white) = auto_levels_preview(&path, image, PREVIEW_MAX_SIDE).expect("auto levels");
    assert!(
        black <= white,
        "auto levels are inverted: {black} > {white}"
    );
    println!("histogram and auto levels: {histogram_time:?}");

    let _ = std::fs::remove_file(&path);
}
