//! Desktop capture (Task 006b).
//!
//! On Windows this wraps DXGI Desktop Duplication: frames of the
//! primary output as BGRA pixels. The pixel-format conversion to
//! I420 (what H.264 encoders eat) is platform-independent and
//! unit-tested; only the duplication plumbing is Windows-gated.
//! Runtime verification needs a real Windows desktop (an
//! interactive session — duplication does not work headless).

/// One captured frame: tightly packed BGRA rows.
#[derive(Debug, Clone, PartialEq)]
pub struct BgraFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Convert a BGRA frame to I420 (BT.601, full-range luma formula
/// commonly used for screen content), planes laid out Y, U, V.
/// Width and height must be even (crop one pixel otherwise).
pub fn bgra_to_i420(frame: &BgraFrame) -> Vec<u8> {
    let w = (frame.width & !1) as usize;
    let h = (frame.height & !1) as usize;
    let mut out = vec![0u8; w * h * 3 / 2];
    let (y_plane, rest) = out.split_at_mut(w * h);
    let (u_plane, v_plane) = rest.split_at_mut(w * h / 4);
    let stride = frame.width as usize * 4;
    for row in 0..h {
        for col in 0..w {
            let i = row * stride + col * 4;
            let (b, g, r) = (
                frame.pixels[i] as i32,
                frame.pixels[i + 1] as i32,
                frame.pixels[i + 2] as i32,
            );
            y_plane[row * w + col] =
                (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
            if row % 2 == 0 && col % 2 == 0 {
                let ci = (row / 2) * (w / 2) + col / 2;
                u_plane[ci] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
                v_plane[ci] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            }
        }
    }
    out
}

/// A simple moving test pattern (colour gradient + sliding bar),
/// used until a real capturer is available and as a fallback when
/// desktop duplication cannot start.
pub fn test_pattern_frame(width: u32, height: u32, tick: u64) -> BgraFrame {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let bar_x = ((tick * 8) % (width as u64 + 160)) as i32 - 80;
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let near_bar = ((x as i32 - bar_x).abs()) < 24;
            pixels[i] = if near_bar {
                240
            } else {
                (x * 255 / width.max(1)) as u8
            };
            pixels[i + 1] = if near_bar {
                240
            } else {
                (y * 255 / height.max(1)) as u8
            };
            pixels[i + 2] = if near_bar { 240 } else { 128 };
            pixels[i + 3] = 255;
        }
    }
    BgraFrame {
        width,
        height,
        pixels,
    }
}

#[cfg(windows)]
pub use windows_impl::DxgiCapturer;

#[cfg(windows)]
mod windows_impl {
    use super::BgraFrame;
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
        DXGI_OUTDUPL_FRAME_INFO,
    };

    /// DXGI Desktop Duplication capturer for the primary output.
    pub struct DxgiCapturer {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        duplication: IDXGIOutputDuplication,
        staging: Option<ID3D11Texture2D>,
        pub width: u32,
        pub height: u32,
    }

    impl DxgiCapturer {
        pub fn new() -> Result<Self, String> {
            unsafe {
                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;
                let levels = [D3D_FEATURE_LEVEL_11_0];
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    windows::Win32::Foundation::HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&levels),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
                .map_err(|e| format!("D3D11CreateDevice: {e}"))?;
                let device = device.ok_or("no D3D11 device")?;
                let context = context.ok_or("no D3D11 context")?;

                let factory: IDXGIFactory1 =
                    CreateDXGIFactory1().map_err(|e| format!("CreateDXGIFactory1: {e}"))?;
                let adapter = factory
                    .EnumAdapters1(0)
                    .map_err(|e| format!("EnumAdapters1: {e}"))?;
                let output = adapter
                    .EnumOutputs(0)
                    .map_err(|e| format!("EnumOutputs: {e}"))?;
                let desc = output.GetDesc().map_err(|e| format!("GetDesc: {e}"))?;
                let output1: IDXGIOutput1 =
                    output.cast().map_err(|e| format!("IDXGIOutput1: {e}"))?;
                let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as u32;
                let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as u32;
                let duplication = output1
                    .DuplicateOutput(&device)
                    .map_err(|e| format!("DuplicateOutput: {e}"))?;
                Ok(Self {
                    device,
                    context,
                    duplication,
                    staging: None,
                    width,
                    height,
                })
            }
        }

        /// Grab the next frame, waiting up to `timeout_ms`.
        /// Returns Ok(None) on timeout (no new frame).
        pub fn capture(&mut self, timeout_ms: u32) -> Result<Option<BgraFrame>, String> {
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<windows::Win32::Graphics::Dxgi::IDXGIResource> = None;
                match self
                    .duplication
                    .AcquireNextFrame(timeout_ms, &mut info, &mut resource)
                {
                    Ok(()) => {}
                    Err(e) => {
                        if e.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAIT_TIMEOUT {
                            return Ok(None);
                        }
                        return Err(format!("AcquireNextFrame: {e}"));
                    }
                }
                let result = self.copy_frame(resource);
                let _ = self.duplication.ReleaseFrame();
                result
            }
        }

        unsafe fn copy_frame(
            &mut self,
            resource: Option<windows::Win32::Graphics::Dxgi::IDXGIResource>,
        ) -> Result<Option<BgraFrame>, String> {
            let resource = resource.ok_or("no frame resource")?;
            let texture: ID3D11Texture2D =
                resource.cast().map_err(|e| format!("frame texture: {e}"))?;
            if self.staging.is_none() {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                texture.GetDesc(&mut desc);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.MiscFlags = 0;
                let mut staging: Option<ID3D11Texture2D> = None;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut staging))
                    .map_err(|e| format!("CreateTexture2D staging: {e}"))?;
                self.staging = staging;
            }
            let staging = self.staging.as_ref().unwrap();
            self.context.CopyResource(staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| format!("Map: {e}"))?;
            let (w, h) = (self.width as usize, self.height as usize);
            let row_pitch = mapped.RowPitch as usize;
            let src = mapped.pData as *const u8;
            let mut pixels = vec![0u8; w * h * 4];
            for row in 0..h {
                std::ptr::copy_nonoverlapping(
                    src.add(row * row_pitch),
                    pixels.as_mut_ptr().add(row * w * 4),
                    w * 4,
                );
            }
            self.context.Unmap(staging, 0);
            Ok(Some(BgraFrame {
                width: self.width,
                height: self.height,
                pixels,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8) -> BgraFrame {
        let mut pixels = vec![0u8; 2 * 2 * 4];
        for i in 0..4 {
            pixels[i * 4] = b;
            pixels[i * 4 + 1] = g;
            pixels[i * 4 + 2] = r;
            pixels[i * 4 + 3] = 255;
        }
        BgraFrame {
            width: 2,
            height: 2,
            pixels,
        }
    }

    #[test]
    fn i420_of_black_and_white() {
        let black = bgra_to_i420(&solid(0, 0, 0));
        assert_eq!(black.len(), 2 * 2 * 3 / 2);
        assert_eq!(black[0], 16); // Y of black
        let white = bgra_to_i420(&solid(255, 255, 255));
        assert_eq!(white[0], 235); // Y of white
        assert_eq!(white[4], 128); // neutral chroma
        assert_eq!(white[5], 128);
    }

    #[test]
    fn i420_handles_saturated_colours_without_overflow() {
        let red = bgra_to_i420(&solid(255, 0, 0));
        assert_eq!(red[0], 82); // Y of pure red (BT.601 limited)
        assert!(red[4] < 128); // U below neutral for red
        assert!(red[5] > 128); // V above neutral for red
        let frame = test_pattern_frame(64, 48, 3);
        assert_eq!(bgra_to_i420(&frame).len(), 64 * 48 * 3 / 2);
    }

    #[test]
    fn test_pattern_has_expected_size_and_moves() {
        let a = test_pattern_frame(64, 48, 0);
        let b = test_pattern_frame(64, 48, 10);
        assert_eq!(a.pixels.len(), 64 * 48 * 4);
        assert_ne!(a.pixels, b.pixels);
    }
}
