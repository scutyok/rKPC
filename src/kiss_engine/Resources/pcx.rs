use anyhow::{anyhow, Result};

pub struct PcxImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
}

pub fn load_pcx(path: &std::path::Path) -> Result<PcxImage> {
    let data = std::fs::read(path)?;
    if data.len() < 128 + 769 {
        return Err(anyhow!("PCX file too small"));
    }
    if data[0] != 0x0A {
        return Err(anyhow!("Not a PCX file"));
    }

    let bits_per_pixel = data[3];
    let xmin = u16::from_le_bytes([data[4], data[5]]) as u32;
    let ymin = u16::from_le_bytes([data[6], data[7]]) as u32;
    let xmax = u16::from_le_bytes([data[8], data[9]]) as u32;
    let ymax = u16::from_le_bytes([data[10], data[11]]) as u32;
    let num_planes = data[65] as u32;
    let bytes_per_line = u16::from_le_bytes([data[66], data[67]]) as usize;

    let width = xmax - xmin + 1;
    let height = ymax - ymin + 1;

    if bits_per_pixel != 8 || num_planes != 1 {
        return Err(anyhow!(
            "Unsupported PCX format: {}bpp, {} planes",
            bits_per_pixel,
            num_planes
        ));
    }

    // Decode RLE image data
    let mut indices = vec![0u8; (height as usize) * bytes_per_line];
    let mut src = 128;
    let mut dst = 0;
    let total = indices.len();

    while dst < total && src < data.len() - 769 {
        let byte = data[src];
        src += 1;
        if byte >= 0xC0 {
            let count = (byte & 0x3F) as usize;
            if src >= data.len() {
                break;
            }
            let value = data[src];
            src += 1;
            let end = (dst + count).min(total);
            for i in dst..end {
                indices[i] = value;
            }
            dst = end;
        } else {
            indices[dst] = byte;
            dst += 1;
        }
    }

    // Read 256-color palette from end of file
    let palette_marker = data[data.len() - 769];
    if palette_marker != 0x0C {
        return Err(anyhow!("Missing PCX palette marker"));
    }
    let palette_start = data.len() - 768;
    let palette = &data[palette_start..];

    // Convert indexed to RGBA
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = indices[y * bytes_per_line + x] as usize;
            pixels.push(palette[idx * 3]);     // R
            pixels.push(palette[idx * 3 + 1]); // G
            pixels.push(palette[idx * 3 + 2]); // B
            pixels.push(255);                   // A
        }
    }

    Ok(PcxImage {
        width,
        height,
        pixels,
    })
}
