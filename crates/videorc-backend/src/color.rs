/// Full-range BT.601 RGB to YUV conversion used by the CPU compositor and by the
/// Metal readback bridge. Keep both paths on this function until the final OBS/HD
/// colorimetry decision is made.
pub fn rgb_to_yuv_full_range_bt601(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = i32::from(r);
    let g = i32::from(g);
    let b = i32::from(b);
    let y = ((77 * r + 150 * g + 29 * b) >> 8).clamp(0, 255) as u8;
    let u = (128 + ((-43 * r - 85 * g + 128 * b) >> 8)).clamp(0, 255) as u8;
    let v = (128 + ((128 * r - 107 * g - 21 * b) >> 8)).clamp(0, 255) as u8;
    (y, u, v)
}

/// Exact inverse of [`rgb_to_yuv_full_range_bt601`]: full-range BT.601 Y'CbCr
/// back to 8-bit RGB. Used by the Linux software preview to turn the CPU
/// compositor's YUV420p output into RGB for JPEG encoding (the macOS preview
/// presents the Metal surface directly and never needs this).
#[allow(dead_code)] // only the Linux preview bridge calls this
#[inline]
pub fn ycbcr_full_range_bt601_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let c = i32::from(y);
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    let r = (c + ((359 * e) >> 8)).clamp(0, 255) as u8;
    let g = (c - ((88 * d) >> 8) - ((183 * e) >> 8)).clamp(0, 255) as u8;
    let b = (c + ((454 * d) >> 8)).clamp(0, 255) as u8;
    (r, g, b)
}

/// BT.709 limited-("video"-)range Y'CbCr to BGR. Capture devices (e.g. an Elgato
/// Cam Link 4K) deliver HD/4K frames as BT.709 video-range Y'CbCr (NV12 `420v`,
/// UYVY `2vuy`); this maps a sample back to 8-bit BGR for the BGRA compositor
/// pipeline. Coefficients are the standard BT.709 limited-range matrix scaled by 256.
#[allow(dead_code)] // used by the macOS camera path in the main bin, not the helper bin
#[inline]
pub fn ycbcr_bt709_video_to_bgr(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let c = i32::from(y) - 16;
    let d = i32::from(cb) - 128;
    let e = i32::from(cr) - 128;
    let r = ((298 * c + 459 * e + 128) >> 8).clamp(0, 255) as u8;
    let g = ((298 * c - 55 * d - 136 * e + 128) >> 8).clamp(0, 255) as u8;
    let b = ((298 * c + 541 * d + 128) >> 8).clamp(0, 255) as u8;
    (b, g, r)
}

/// BT.709 full-range Y'CbCr to BGR, for full-range NV12 (`420f`).
#[allow(dead_code)] // used by the macOS camera path in the main bin, not the helper bin
#[inline]
pub fn ycbcr_bt709_full_to_bgr(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let c = i32::from(y);
    let d = i32::from(cb) - 128;
    let e = i32::from(cr) - 128;
    let r = ((256 * c + 403 * e + 128) >> 8).clamp(0, 255) as u8;
    let g = ((256 * c - 48 * d - 120 * e + 128) >> 8).clamp(0, 255) as u8;
    let b = ((256 * c + 475 * d + 128) >> 8).clamp(0, 255) as u8;
    (b, g, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycbcr_bt709_video_maps_black_and_white() {
        // Video-range luma endpoints with neutral chroma -> pure black / white.
        assert_eq!(ycbcr_bt709_video_to_bgr(16, 128, 128), (0, 0, 0));
        assert_eq!(ycbcr_bt709_video_to_bgr(235, 128, 128), (255, 255, 255));
    }

    #[test]
    fn ycbcr_bt709_video_maps_primaries() {
        // BT.709 video-range encodings of pure R/G/B decode back to those primaries
        // (within rounding). Order is (B, G, R).
        let (b, g, r) = ycbcr_bt709_video_to_bgr(63, 102, 240); // red
        assert!(r >= 250 && g <= 4 && b <= 4, "red -> {b},{g},{r}");
        let (b, _g, r) = ycbcr_bt709_video_to_bgr(173, 42, 26); // green
        assert!(r <= 6 && b <= 6, "green -> {b},{r}");
        let (b, g, r) = ycbcr_bt709_video_to_bgr(32, 240, 118); // blue
        assert!(b >= 250 && g <= 6 && r <= 6, "blue -> {b},{g},{r}");
    }

    #[test]
    fn ycbcr_bt709_full_maps_black_and_white() {
        assert_eq!(ycbcr_bt709_full_to_bgr(0, 128, 128), (0, 0, 0));
        assert_eq!(ycbcr_bt709_full_to_bgr(255, 128, 128), (255, 255, 255));
    }

    #[test]
    fn rgb_to_yuv_full_range_bt601_matches_existing_primary_references() {
        assert_eq!(rgb_to_yuv_full_range_bt601(255, 0, 0), (76, 85, 255));
        assert_eq!(rgb_to_yuv_full_range_bt601(0, 255, 0), (149, 43, 21));
        assert_eq!(rgb_to_yuv_full_range_bt601(0, 0, 255), (28, 255, 107));
    }

    #[test]
    fn rgb_to_yuv_full_range_bt601_maps_black_and_white_to_full_range_luma() {
        assert_eq!(rgb_to_yuv_full_range_bt601(0, 0, 0), (0, 128, 128));
        assert_eq!(rgb_to_yuv_full_range_bt601(255, 255, 255), (255, 128, 128));
    }

    #[test]
    fn ycbcr_full_range_bt601_inverts_the_forward_conversion() {
        // Black / white endpoints exact.
        assert_eq!(ycbcr_full_range_bt601_to_rgb(0, 128, 128), (0, 0, 0));
        assert_eq!(
            ycbcr_full_range_bt601_to_rgb(255, 128, 128),
            (255, 255, 255)
        );
        // Primaries round-trip within integer-math rounding tolerance.
        for (r, g, b) in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (120, 200, 40)] {
            let (y, u, v) = rgb_to_yuv_full_range_bt601(r, g, b);
            let (r2, g2, b2) = ycbcr_full_range_bt601_to_rgb(y, u, v);
            assert!(
                (i32::from(r) - i32::from(r2)).abs() <= 4
                    && (i32::from(g) - i32::from(g2)).abs() <= 4
                    && (i32::from(b) - i32::from(b2)).abs() <= 4,
                "round-trip {r},{g},{b} -> {r2},{g2},{b2}"
            );
        }
    }
}
