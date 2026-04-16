use anyhow::Result;

/// Convert raw CF_DIB data (BITMAPINFOHEADER + pixel data, no BMP file header) to PNG bytes.
///
/// This function is platform-independent (uses only the `image` crate) and can be tested
/// on any OS. Windows-specific clipboard access is handled separately in `platform/windows.rs`.
pub(crate) fn dib_to_png(dib_data: &[u8]) -> Result<Vec<u8>> {
    use image::codecs::bmp::BmpDecoder;
    use image::DynamicImage;
    use std::io::Cursor;

    let cursor = Cursor::new(dib_data);
    let decoder = BmpDecoder::new_without_file_header(cursor)
        .map_err(|e| anyhow::anyhow!("Failed to decode DIB: {}", e))?;
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|e| anyhow::anyhow!("Failed to load DIB image: {}", e))?;

    let mut png_bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("Failed to encode PNG: {}", e))?;

    Ok(png_bytes)
}
