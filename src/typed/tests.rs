//! Tests for type-safe JPEG encoding API

use super::*;

/// Create a simple test pattern (gradient)
fn create_test_pattern(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width) as u8;
            let g = (y * 255 / height) as u8;
            let b = 128u8;
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
        }
    }
    pixels
}

#[test]
fn simple_rgb_encoding() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let jpeg = config.encode_rgb(&pixels, width, height)
        .expect("Failed to encode JPEG");

    assert!(!jpeg.is_empty());
    assert!(jpeg.len() < pixels.len()); // Should be compressed
}

#[test]
fn scanline_encoder_basic() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn scanline_encoder_with_smoothing() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig {
        input: JpegInput::Scanlines(
            ScanlineConfig::RgbToYCbCr {
                subsampling: ChromaSubsampling::Yuv420,
                smoothing: 50,
            }
        ),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn progressive_encoding() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig {
        input: JpegInput::Scanlines(
            ScanlineConfig::RgbToYCbCr {
                subsampling: ChromaSubsampling::Yuv420,
                smoothing: 0,
            }
        ),
        compression: CompressionMode::Progressive {
            optimize_scans: true,
            use_scans_in_trellis: false,
        },
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn yuv444_no_subsampling() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig {
        input: JpegInput::Scanlines(
            ScanlineConfig::RgbToYCbCr {
                subsampling: ChromaSubsampling::Yuv444,
                smoothing: 0,
            }
        ),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn grayscale_encoding() {
    let width = 64;
    let height = 64;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for _x in 0..width {
            pixels.push((y * 255 / height) as u8);
        }
    }

    let config = JpegConfig::grayscale(85.0);
    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn config_validation_invalid_dimensions() {
    let config = JpegConfig {
        input: JpegInput::Scanlines(ScanlineConfig::Grayscale),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    // Dimensions are now validated at encoder creation time
    let result = config.create_encoder(0, 0);
    assert!(result.is_err());
    match result {
        Err(ConfigError::InvalidDimensions { .. }) => {}
        _ => panic!("Expected InvalidDimensions error"),
    }
}

#[test]
fn config_validation_invalid_quality() {
    let config = JpegConfig {
        input: JpegInput::Scanlines(ScanlineConfig::Grayscale),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 150.0,  // Invalid!
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    let result = config.validate();
    assert!(result.is_err());
    match result {
        Err(ConfigError::InvalidQuality(_)) => {}
        _ => panic!("Expected InvalidQuality error"),
    }
}

#[test]
fn config_builder_style() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::rgb_to_ycbcr_420(85.0)
        .with_progressive()
        .with_smoothing(25);

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn chroma_subsampling_mcu_sizes() {
    assert_eq!(ChromaSubsampling::Yuv444.mcu_size(), (8, 8));
    assert_eq!(ChromaSubsampling::Yuv422.mcu_size(), (16, 8));
    assert_eq!(ChromaSubsampling::Yuv420.mcu_size(), (16, 16));
    assert_eq!(ChromaSubsampling::Yuv411.mcu_size(), (32, 8));
}

#[test]
fn chroma_subsampling_chroma_sizes() {
    // 4:4:4 - no subsampling
    assert_eq!(ChromaSubsampling::Yuv444.chroma_size(640, 480), (640, 480));

    // 4:2:2 - horizontal subsampling
    assert_eq!(ChromaSubsampling::Yuv422.chroma_size(640, 480), (320, 480));

    // 4:2:0 - both directions
    assert_eq!(ChromaSubsampling::Yuv420.chroma_size(640, 480), (320, 240));

    // 4:1:1 - aggressive horizontal
    assert_eq!(ChromaSubsampling::Yuv411.chroma_size(640, 480), (160, 480));
}

#[test]
fn config_convenience_constructors() {
    // Test all convenience constructors compile and validate
    let _width = 64;
    let _height = 64;

    let config1 = JpegConfig::rgb_to_ycbcr_420(85.0);
    assert!(config1.validate().is_ok());

    let config2 = JpegConfig::rgb_to_ycbcr_444(85.0);
    assert!(config2.validate().is_ok());

    let config3 = JpegConfig::rgb_to_rgb(85.0);
    assert!(config3.validate().is_ok());

    let config4 = JpegConfig::grayscale(85.0);
    assert!(config4.validate().is_ok());
}

#[test]
fn raw_mcu_validation_invalid_dimensions() {
    // 641 is not a multiple of 16 (MCU size for 4:2:0)
    let config = JpegConfig {
        input: JpegInput::RawMcu(
            RawMcuConfig::YCbCr {
                subsampling: ChromaSubsampling::Yuv420,
                y_size: (641, 480),
                cb_size: (320, 240),
                cr_size: (320, 240),
            }
        ),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };

    // Dimensions are now validated at encoder creation time
    let result = config.create_mcu_planar_encoder(641, 480);
    assert!(result.is_err());
    match result {
        Err(ConfigError::InvalidMcuDimensions { .. }) => {}
        _ => panic!("Expected InvalidMcuDimensions error"),
    }
}

#[test]
fn scanline_config_color_spaces() {
    assert_eq!(
        ScanlineConfig::RgbToYCbCr {
            subsampling: ChromaSubsampling::Yuv420,
            smoothing: 0,
        }.input_color_space(),
        crate::ColorSpace::JCS_RGB
    );

    assert_eq!(
        ScanlineConfig::RgbToYCbCr {
            subsampling: ChromaSubsampling::Yuv420,
            smoothing: 0,
        }.jpeg_color_space(),
        crate::ColorSpace::JCS_YCbCr
    );

    assert_eq!(
        ScanlineConfig::RgbToRgb.input_color_space(),
        crate::ColorSpace::JCS_RGB
    );

    assert_eq!(
        ScanlineConfig::RgbToRgb.jpeg_color_space(),
        crate::ColorSpace::JCS_RGB
    );

    assert_eq!(
        ScanlineConfig::Grayscale.input_color_space(),
        crate::ColorSpace::JCS_GRAYSCALE
    );
}

#[test]
fn preset_sequential_fastest() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::from_preset(Preset::SequentialFastest, 75.0);
    assert!(config.validate().is_ok());
    assert_eq!(config.quality, 75.0);
    assert!(!config.optimize_coding);
    assert!(matches!(config.compression, CompressionMode::Sequential));

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");
    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn preset_sequential_balanced() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::from_preset(Preset::SequentialBalanced, 85.0);
    assert!(config.validate().is_ok());
    assert_eq!(config.quality, 85.0);
    assert!(config.optimize_coding);
    assert!(matches!(config.compression, CompressionMode::Sequential));

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");
    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn preset_progressive_balanced() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    assert!(config.validate().is_ok());
    assert_eq!(config.quality, 85.0);
    assert!(config.optimize_coding);
    assert!(matches!(
        config.compression,
        CompressionMode::Progressive { optimize_scans: false, use_scans_in_trellis: false }
    ));

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");
    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn preset_progressive_smallest() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::from_preset(Preset::ProgressiveSmallest, 75.0);
    assert!(config.validate().is_ok());
    assert_eq!(config.quality, 75.0);
    assert!(config.optimize_coding);
    assert!(matches!(
        config.compression,
        CompressionMode::Progressive { optimize_scans: true, use_scans_in_trellis: true }
    ));

    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");
    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines(&pixels).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}

#[test]
fn presets_produce_different_configs() {
    // Verify that presets produce different configurations
    let _width = 256;
    let _height = 256;
    let quality = 85.0;

    let fastest = JpegConfig::from_preset(Preset::SequentialFastest, quality);
    let sequential = JpegConfig::from_preset(Preset::SequentialBalanced, quality);
    let progressive = JpegConfig::from_preset(Preset::ProgressiveBalanced, quality);
    let smallest = JpegConfig::from_preset(Preset::ProgressiveSmallest, quality);

    // All should use the same quality
    assert_eq!(fastest.quality, quality);
    assert_eq!(sequential.quality, quality);
    assert_eq!(progressive.quality, quality);
    assert_eq!(smallest.quality, quality);

    // Verify optimization flags
    assert!(!fastest.optimize_coding);
    assert!(sequential.optimize_coding);
    assert!(progressive.optimize_coding);
    assert!(smallest.optimize_coding);

    // Verify compression modes
    assert!(matches!(fastest.compression, CompressionMode::Sequential));
    assert!(matches!(sequential.compression, CompressionMode::Sequential));
    assert!(matches!(
        progressive.compression,
        CompressionMode::Progressive { optimize_scans: false, .. }
    ));
    assert!(matches!(
        smallest.compression,
        CompressionMode::Progressive { optimize_scans: true, .. }
    ));

    // All use 4:2:0 subsampling by default
    for config in &[&fastest, &sequential, &progressive, &smallest] {
        match &config.input {
            JpegInput::Scanlines(ScanlineConfig::RgbToYCbCr { subsampling, .. }) => {
                assert_eq!(*subsampling, ChromaSubsampling::Yuv420);
            }
            _ => panic!("Expected RgbToYCbCr scanline config"),
        }
    }
}

#[test]
fn presets_all_encode_successfully() {
    // Verify all presets can encode successfully
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let presets = [
        Preset::SequentialFastest,
        Preset::SequentialBalanced,
        Preset::ProgressiveBalanced,
        Preset::ProgressiveSmallest,
    ];

    for preset in &presets {
        let config = JpegConfig::from_preset(*preset, 85.0);
        assert!(config.validate().is_ok());

        let encoder = config.create_encoder(width, height).expect("Failed to create encoder");
        let mut started = encoder.start(Vec::new()).expect("Failed to start");
        started.write_scanlines(&pixels).expect("Failed to write");
        let jpeg = started.finish().expect("Failed to finish");

        assert!(!jpeg.is_empty(), "Preset {:?} produced empty output", preset);
        assert!(jpeg.len() < pixels.len(), "Preset {:?} didn't compress", preset);
    }
}

#[test]
fn scanline_encoder_with_stride() {
    let width = 64;
    let height = 64;

    // Create data with stride (extra padding per row)
    let components = 3; // RGB
    let row_width = width * components;
    let stride = row_width + 16; // Add 16 bytes padding per row
    let mut pixels = vec![0u8; stride * height];

    // Fill with test pattern (only the actual pixels, not padding)
    for y in 0..height {
        for x in 0..width {
            let offset = y * stride + x * components;
            pixels[offset] = (x * 255 / width) as u8;     // R
            pixels[offset + 1] = (y * 255 / height) as u8; // G
            pixels[offset + 2] = 128;                      // B
        }
    }

    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_scanlines_strided(&pixels, stride).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
    assert!(jpeg.len() < pixels.len()); // Should be compressed
}

#[cfg(feature = "image_ref")]
#[test]
fn encode_imgref() {
    use imgref::ImgVec;

    let width = 64;
    let height = 64;

    // Create an ImgVec with RGB8 data
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            pixels.push((x * 255 / width) as u8);     // R
            pixels.push((y * 255 / height) as u8);   // G
            pixels.push(128u8);                       // B
        }
    }

    // Reinterpret as RGB triples
    let rgb_pixels: Vec<[u8; 3]> = pixels.chunks_exact(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect();

    let img = ImgVec::new(rgb_pixels, width, height);

    // Encode using imgref - works with ImgVec via .as_ref()
    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let jpeg = config.encode_imgref(&img.as_ref())
        .expect("Failed to encode");

    assert!(!jpeg.is_empty());
}

#[cfg(feature = "image_ref")]
#[test]
fn encode_imgref_with_stride() {
    use imgref::ImgVec;

    let width = 64;
    let height = 64;
    let stride = 80; // Wider than needed (has padding)

    // Create buffer with stride
    let mut pixels = vec![[0u8, 0u8, 0u8]; stride * height];
    for y in 0..height {
        for x in 0..width {
            let idx = y * stride + x;
            pixels[idx] = [
                (x * 255 / width) as u8,
                (y * 255 / height) as u8,
                128u8,
            ];
        }
    }

    // Use new_stride to explicitly set the stride
    let img = ImgVec::new_stride(pixels, width, height, stride);

    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let encoder = config.create_encoder(width, height).expect("Failed to create encoder");

    let mut started = encoder.start(Vec::new()).expect("Failed to start");
    started.write_imgref(&img.as_ref()).expect("Failed to write");
    let jpeg = started.finish().expect("Failed to finish");

    assert!(!jpeg.is_empty());
}
// ============================================================================
// Parity Tests: Typed API vs Legacy API
// ============================================================================
//
// These tests ensure that the typed API produces identical output to the
// legacy API for equivalent configurations.

/// Helper to create JPEG using legacy API
fn encode_legacy_rgb_to_ycbcr_420(
    pixels: &[u8],
    width: usize,
    height: usize,
    quality: f32,
    progressive: bool,
    optimize_coding: bool,
) -> Vec<u8> {
    use crate::{Compress, ColorSpace, ScanMode};

    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(ScanMode::Auto);
    comp.set_quality(quality);
    comp.set_optimize_coding(optimize_coding);

    // Set 4:2:0 subsampling
    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    if progressive {
        comp.set_progressive_mode();
    }

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(pixels).unwrap();
    started.finish().unwrap()
}

#[test]
fn parity_sequential_fastest() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::from_preset(Preset::SequentialFastest, 85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - SequentialFastest = no progressive, no optimize_coding
    let legacy_jpeg = encode_legacy_rgb_to_ycbcr_420(&pixels, width, height, 85.0, false, false);

    // Should produce identical output
    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_sequential_balanced() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::from_preset(Preset::SequentialBalanced, 85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - SequentialBalanced = no progressive, yes optimize_coding
    let legacy_jpeg = encode_legacy_rgb_to_ycbcr_420(&pixels, width, height, 85.0, false, true);

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_progressive_balanced() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - ProgressiveBalanced = progressive, optimize_coding, no optimize_scans
    let legacy_jpeg = encode_legacy_rgb_to_ycbcr_420(&pixels, width, height, 85.0, true, true);

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_progressive_smallest() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::from_preset(Preset::ProgressiveSmallest, 85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - ProgressiveSmallest adds optimize_scans and use_scans_in_trellis
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);
    comp.set_progressive_mode();
    comp.set_optimize_scans(true);
    comp.set_use_scans_in_trellis(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_yuv444_no_subsampling() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::rgb_to_ycbcr_444(85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - 4:4:4 (no subsampling)
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 1;
    comps[0].v_samp_factor = 1;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_yuv422_subsampling() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig {
        input: JpegInput::Scanlines(
            ScanlineConfig::RgbToYCbCr {
                subsampling: ChromaSubsampling::Yuv422,
                smoothing: 0,
            }
        ),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API - 4:2:2
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 1;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_grayscale() {
    let width = 64;
    let height = 64;
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for _x in 0..width {
            pixels.push((y * 255 / height) as u8);
        }
    }

    // Typed API
    let config = JpegConfig::grayscale(85.0);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_GRAYSCALE);
    comp.set_size(width, height);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_with_smoothing() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::rgb_to_ycbcr_420(85.0).with_smoothing(50);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API — set_scan_optimization_mode must come before smoothing
    // because it calls jpeg_set_defaults() which resets smoothing to 0
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);
    comp.set_smoothing_factor(50);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_different_qualities() {
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    for quality in [60.0, 75.0, 85.0, 95.0] {
        // Typed API
        let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, quality);
        let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

        // Legacy API
        let legacy_jpeg = encode_legacy_rgb_to_ycbcr_420(&pixels, width, height, quality, true, true);

        assert_eq!(
            typed_jpeg.len(),
            legacy_jpeg.len(),
            "File sizes differ at quality {}",
            quality
        );
        assert_eq!(
            typed_jpeg,
            legacy_jpeg,
            "JPEG bytes differ at quality {}",
            quality
        );
    }
}

#[test]
fn parity_stride_support() {
    let width = 64;
    let height = 64;
    let stride = 192 + 16; // RGB width + padding

    // Create strided pixel data
    let mut pixels = vec![0u8; stride * height];
    for y in 0..height {
        for x in 0..width {
            let offset = y * stride + x * 3;
            pixels[offset] = (x * 255 / width) as u8;
            pixels[offset + 1] = (y * 255 / height) as u8;
            pixels[offset + 2] = 128u8;
        }
    }

    // Typed API
    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    let typed_jpeg = config.encode_rgb_strided(&pixels, width, height, stride).unwrap();

    // Legacy API
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines_strided(&pixels, stride).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn parity_raw_mcu_mode() {
    let width = 64;
    let height = 64;

    // Create YCbCr component data (4:2:0)
    let y_plane = vec![128u8; width * height];
    let cb_plane = vec![128u8; (width / 2) * (height / 2)];
    let cr_plane = vec![128u8; (width / 2) * (height / 2)];

    // Typed API
    let config = JpegConfig {
        input: JpegInput::RawMcu(
            RawMcuConfig::YCbCr {
                subsampling: ChromaSubsampling::Yuv420,
                y_size: (width, height),
                cb_size: (width / 2, height / 2),
                cr_size: (width / 2, height / 2),
            }
        ),
        compression: CompressionMode::Sequential,
        qtables: QTableConfig::FromQuality,
        quality: 85.0,
        optimize_coding: true,
        scan_mode: crate::ScanMode::Auto,
        force_8bit_quantization: false,
    };
    let encoder = config.create_mcu_planar_encoder(width, height).unwrap();
    let mut started = encoder.start(Vec::new()).unwrap();
    started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane]).unwrap();
    let typed_jpeg = started.finish().unwrap();

    // Legacy API
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_YCbCr);
    comp.set_size(width, height);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality(85.0);
    comp.set_optimize_coding(true);
    comp.set_raw_data_in(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane]);
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

// ============================================================================
// 8-bit Quantization Tests
// ============================================================================

#[test]
fn force_8bit_produces_different_output_at_low_quality() {
    // At very low quality, quantization values can exceed 255.
    // force_8bit_quantization clamps them to 255, producing different output.
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Without 8-bit clamping (default)
    let config_normal = JpegConfig::from_preset(Preset::SequentialBalanced, 10.0);
    let jpeg_normal = config_normal.encode_rgb(&pixels, width, height).unwrap();

    // With 8-bit clamping
    let config_clamped = JpegConfig::from_preset(Preset::SequentialBalanced, 10.0)
        .with_force_8bit_quantization(true);
    let jpeg_clamped = config_clamped.encode_rgb(&pixels, width, height).unwrap();

    // At quality 10, some qtable values exceed 255, so clamping changes the output
    assert_ne!(jpeg_normal, jpeg_clamped,
        "force_8bit_quantization should produce different output at low quality");
}

#[test]
fn force_8bit_no_difference_at_high_quality() {
    // At high quality, quantization values are all <= 255 already,
    // so force_8bit_quantization makes no difference.
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config_normal = JpegConfig::from_preset(Preset::SequentialBalanced, 85.0);
    let jpeg_normal = config_normal.encode_rgb(&pixels, width, height).unwrap();

    let config_clamped = JpegConfig::from_preset(Preset::SequentialBalanced, 85.0)
        .with_force_8bit_quantization(true);
    let jpeg_clamped = config_clamped.encode_rgb(&pixels, width, height).unwrap();

    // At quality 85, all qtable values are <= 255, so output is identical
    assert_eq!(jpeg_normal, jpeg_clamped,
        "force_8bit_quantization should not affect output at high quality");
}

#[test]
fn force_8bit_works_with_progressive() {
    // force_8bit_quantization only clamps qtable values — it does not disable progressive mode
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 10.0)
        .with_force_8bit_quantization(true);

    let jpeg = config.encode_rgb(&pixels, width, height).unwrap();
    assert!(!jpeg.is_empty());

    // Verify it's still progressive by checking for SOS markers (multiple scans)
    // A progressive JPEG has multiple SOS markers (0xFF 0xDA)
    let sos_count = jpeg.windows(2)
        .filter(|w| w[0] == 0xFF && w[1] == 0xDA)
        .count();
    assert!(sos_count > 1, "Expected progressive JPEG (multiple SOS markers), got {}", sos_count);
}

#[test]
fn force_8bit_parity_with_legacy() {
    // Typed API with force_8bit_quantization should match legacy set_quality_force_8bit
    let width = 64;
    let height = 64;
    let pixels = create_test_pattern(width, height);

    // Typed API
    let config = JpegConfig::from_preset(Preset::SequentialBalanced, 10.0)
        .with_force_8bit_quantization(true);
    let typed_jpeg = config.encode_rgb(&pixels, width, height).unwrap();

    // Legacy API
    let mut comp = crate::Compress::new(crate::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_color_space(crate::ColorSpace::JCS_YCbCr);
    comp.set_scan_optimization_mode(crate::ScanMode::Auto);
    comp.set_quality_force_8bit(10.0, true);
    comp.set_optimize_coding(true);

    let comps = comp.components_mut();
    comps[0].h_samp_factor = 2;
    comps[0].v_samp_factor = 2;
    comps[1].h_samp_factor = 1;
    comps[1].v_samp_factor = 1;
    comps[2].h_samp_factor = 1;
    comps[2].v_samp_factor = 1;

    let mut started = comp.start_compress(Vec::new()).unwrap();
    started.write_scanlines(&pixels).unwrap();
    let legacy_jpeg = started.finish().unwrap();

    assert_eq!(typed_jpeg.len(), legacy_jpeg.len(), "File sizes differ");
    assert_eq!(typed_jpeg, legacy_jpeg, "JPEG bytes differ");
}

#[test]
fn force_8bit_default_is_false() {
    let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    assert!(!config.force_8bit_quantization);

    let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    assert!(!config.force_8bit_quantization);

    let config = JpegConfig::grayscale(85.0);
    assert!(!config.force_8bit_quantization);
}

#[test]
fn force_8bit_builder_method() {
    let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0)
        .with_force_8bit_quantization(true);
    assert!(config.force_8bit_quantization);

    let config = config.with_force_8bit_quantization(false);
    assert!(!config.force_8bit_quantization);
}


