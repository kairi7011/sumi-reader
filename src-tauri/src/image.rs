use base64::{engine::general_purpose::STANDARD, Engine};

const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

pub fn decode_png(data_url: &str) -> Result<Vec<u8>, String> {
    if data_url.len() > MAX_IMAGE_BYTES * 4 / 3 + 100 { return Err("画像が大きすぎます。".into()); }
    let encoded = data_url.strip_prefix("data:image/png;base64,").ok_or("PNG画像を読み込めませんでした。")?;
    let bytes = STANDARD.decode(encoded).map_err(|_| "PNG画像を読み込めませんでした。")?;
    if bytes.len() > MAX_IMAGE_BYTES || bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" { return Err("PNG画像を読み込めませんでした。".into()); }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as u64;
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap()) as u64;
    if width == 0 || height == 0 || width > 12000 || height > 12000 || width * height > 32_000_000 { return Err("画像が大きすぎます。".into()); }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_decoder_rejects_other_formats_and_oversized_dimensions() {
        assert!(decode_png("data:image/jpeg;base64,AAAA").is_err());
        assert!(decode_png("data:image/png;base64,aGVsbG8=").is_err());
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&12001u32.to_be_bytes());
        png.extend_from_slice(&1u32.to_be_bytes());
        assert!(decode_png(&format!("data:image/png;base64,{}", STANDARD.encode(png))).is_err());
    }
}
