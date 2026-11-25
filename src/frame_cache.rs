use std::{fs::File, io::Read, path::Path};
use windows::Win32::Storage::Compression::{
    COMPRESS_ALGORITHM_XPRESS, CloseDecompressor, CreateDecompressor, Decompress,
};

pub struct CachedFrame {
    pub size: wgpu::Extent3d,
    pub format: wgpu::TextureFormat,
    pub pixels: Vec<u8>,
}

impl CachedFrame {
    pub fn read(path: &Path) -> crate::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Self::decode(file, len)
    }

    fn decode(mut file: impl Read, len: u64) -> crate::Result<Self> {
        let mut header = [0u8; 20];
        file.read_exact(&mut header)?;
        let field =
            |offset: usize| u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap());
        let (width, height, payload) = (field(4), field(8), field(16) as usize);
        if &header[..4] != b"IKF1" || !(1..=8192).contains(&width) || !(1..=8192).contains(&height)
        {
            return Err("Cabecera de fotograma inválida".into());
        }
        let format = match field(12) {
            28 => wgpu::TextureFormat::Rgba8Unorm,
            87 => wgpu::TextureFormat::Bgra8Unorm,
            _ => return Err("Formato de fotograma inválido".into()),
        };
        let bytes = width as usize * height as usize * 4;
        if payload == 0 || payload > bytes || len != 20 + payload as u64 {
            return Err("Tamaño de fotograma inválido".into());
        }
        let mut pixels = vec![0; payload];
        file.read_exact(&mut pixels)?;
        if payload < bytes {
            let mut decoded = vec![0u8; bytes];
            let mut size = 0;
            unsafe {
                let mut handle = Default::default();
                CreateDecompressor(COMPRESS_ALGORITHM_XPRESS, None, &mut handle)?;
                let result = Decompress(
                    handle,
                    Some(pixels.as_ptr().cast()),
                    payload,
                    Some(decoded.as_mut_ptr().cast()),
                    bytes,
                    Some(&mut size),
                );
                let _ = CloseDecompressor(handle);
                result?;
            }
            if size != bytes {
                return Err("Fotograma descomprimido incompleto".into());
            }
            pixels = decoded;
        }
        Ok(Self {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            format,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Storage::Compression::{CloseCompressor, Compress, CreateCompressor};

    #[test]
    fn preserves_native_bgra_pixels_and_rejects_truncation() -> crate::Result<()> {
        let expected = [31u8, 89, 123, 255].repeat(64 * 32);
        let mut packed = vec![0u8; expected.len()];
        let mut size = 0;
        unsafe {
            let mut handle = Default::default();
            CreateCompressor(COMPRESS_ALGORITHM_XPRESS, None, &mut handle)?;
            let result = Compress(
                handle,
                Some(expected.as_ptr().cast()),
                expected.len(),
                Some(packed.as_mut_ptr().cast()),
                packed.len(),
                &mut size,
            );
            let _ = CloseCompressor(handle);
            result?;
        }
        assert!(size < expected.len());
        for payload in [&packed[..size], &expected[..]] {
            let mut bytes = b"IKF1".to_vec();
            for field in [64u32, 32, 87, payload.len() as u32] {
                bytes.extend(field.to_le_bytes());
            }
            bytes.extend(payload);
            let frame = CachedFrame::decode(&bytes[..], bytes.len() as u64)?;
            assert_eq!((frame.size.width, frame.size.height), (64, 32));
            assert_eq!(frame.format, wgpu::TextureFormat::Bgra8Unorm);
            assert_eq!(&frame.pixels, &expected);
            bytes.pop();
            assert!(CachedFrame::decode(&bytes[..], bytes.len() as u64).is_err());
        }
        Ok(())
    }
}
