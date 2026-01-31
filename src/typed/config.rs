//! Type-safe JPEG configuration types
//!
//! This module defines the configuration types for the type-safe JPEG encoding API.
//! See the parent `typed` module for usage examples.

use std::io;
use crate::{ColorSpace, ScanMode};
use crate::qtable::QTable;

// Re-exports for convenience
pub use crate::density::PixelDensity;

/// Complete type-safe JPEG configuration
///
/// Note: Dimensions are passed at encode time, not stored in config.
/// This allows the same config to be reused for multiple images.
#[derive(Clone, Debug)]
pub struct JpegConfig {
    /// Input mode (scanline vs raw MCU) - encodes valid color space combinations
    pub input: JpegInput,

    /// Compression mode (sequential vs progressive)
    pub compression: CompressionMode,

    /// Quantization table configuration
    pub qtables: QTableConfig,

    /// Quality parameter (0-100, lower = smaller file, higher = better quality)
    pub quality: f32,

    /// Huffman coding optimization (recommended: true)
    pub optimize_coding: bool,

    /// Scan optimization mode (MozJPEG specific)
    pub scan_mode: ScanMode,

    /// Clamp quantization table values to 1-255 (8-bit range).
    ///
    /// When `true`, all quantization table entries are clamped to the range 1-255.
    /// This produces DQT markers with 8-bit precision, which is the most
    /// widely compatible format.
    ///
    /// When `false` (the default), table values can go up to 32767 and the
    /// encoder uses 16-bit DQT precision when needed. At low quality settings
    /// (below ~50), many quantization values exceed 255, so the default
    /// produces 16-bit DQT entries. All compliant JPEG decoders handle this.
    ///
    /// **Note**: This only affects quantization table value range. It does NOT
    /// disable progressive encoding, change the SOF marker type, or affect any
    /// other encoding parameters.
    ///
    /// Corresponds to the `force_baseline` parameter in libjpeg's
    /// `jpeg_set_quality()` and `jpeg_add_quant_table()`.
    ///
    /// **Default**: `false` (allows 16-bit quantization values)
    pub force_8bit_quantization: bool,
}

/// Encoding presets that balance speed, file size, and quality.
///
/// These presets determine the encoding mode and optimization level, but NOT
/// the quality value - set quality separately using `with_quality()` or in
/// `from_preset()`.
///
/// Matches the preset design from mozjpeg-rs for consistency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Fastest encoding with no optimizations.
    ///
    /// - **Progressive**: No
    /// - **Huffman opt**: No
    /// - **Optimize scans**: No
    ///
    /// Files are ~10-20% larger than optimized modes, but encoding is faster.
    /// Use for real-time encoding, thumbnails, or when speed is critical.
    ///
    /// **File size**: Largest
    SequentialFastest,

    /// Sequential JPEG with Huffman optimization.
    ///
    /// - **Progressive**: No
    /// - **Huffman opt**: Yes
    ///
    /// Best for applications requiring sequential decode order (video players,
    /// legacy systems) while still achieving good compression.
    ///
    /// **File size**: Good
    SequentialBalanced,

    /// Progressive JPEG with optimizations (recommended default).
    ///
    /// - **Progressive**: Yes
    /// - **Huffman opt**: Yes
    /// - **Optimize scans**: No
    ///
    /// Good balance of size, quality, and encoding speed. Progressive rendering
    /// provides better perceived loading experience for web images.
    ///
    /// Note: Does NOT include `optimize_scans` (which adds ~100% encoding time
    /// for only ~1% additional size reduction).
    ///
    /// **Encoding time**: ~2x
    /// **File size**: Good
    ProgressiveBalanced,

    /// Maximum compression (matches mozjpeg defaults).
    ///
    /// - **Progressive**: Yes
    /// - **Huffman opt**: Yes
    /// - **Optimize scans**: Yes
    ///
    /// Tries multiple progressive scan configurations to find the smallest output.
    /// This adds ~100% encoding time for only ~1% additional size reduction.
    ///
    /// Use when file size is critical and encoding time is not.
    ///
    /// **Encoding time**: ~4x (twice as slow as ProgressiveBalanced)
    /// **File size**: Smallest
    ProgressiveSmallest,
}

impl Preset {
    /// Returns true if this preset uses progressive encoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::Preset;
    /// assert!(!Preset::SequentialFastest.is_progressive());
    /// assert!(Preset::ProgressiveBalanced.is_progressive());
    /// ```
    pub const fn is_progressive(self) -> bool {
        matches!(
            self,
            Preset::ProgressiveBalanced | Preset::ProgressiveSmallest
        )
    }

    /// Returns true if Huffman table optimization is enabled.
    ///
    /// Huffman optimization computes custom Huffman tables for the image rather
    /// than using default tables. This improves compression by ~5-10% with minimal
    /// encoding overhead.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::Preset;
    /// assert!(!Preset::SequentialFastest.has_huffman_opt());
    /// assert!(Preset::SequentialBalanced.has_huffman_opt());
    /// ```
    pub const fn has_huffman_opt(self) -> bool {
        !matches!(self, Preset::SequentialFastest)
    }

    /// Returns true if optimize_scans is enabled.
    ///
    /// Scan optimization tries multiple progressive scan configurations to find
    /// the smallest output. Only enabled for `ProgressiveSmallest`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::Preset;
    /// assert!(!Preset::ProgressiveBalanced.has_optimize_scans());
    /// assert!(Preset::ProgressiveSmallest.has_optimize_scans());
    /// ```
    pub const fn has_optimize_scans(self) -> bool {
        matches!(self, Preset::ProgressiveSmallest)
    }

    /// Get the compression mode for this preset
    fn compression_mode(self) -> CompressionMode {
        match self {
            Preset::SequentialFastest | Preset::SequentialBalanced => CompressionMode::Sequential,
            Preset::ProgressiveBalanced => CompressionMode::Progressive {
                optimize_scans: false,
                use_scans_in_trellis: false,
            },
            Preset::ProgressiveSmallest => CompressionMode::Progressive {
                optimize_scans: true,
                use_scans_in_trellis: true,
            },
        }
    }

    /// Get whether to enable Huffman coding optimization
    fn optimize_coding(self) -> bool {
        self.has_huffman_opt()
    }
}

impl JpegConfig {
    /// Create config from a preset with specified quality.
    ///
    /// The preset determines encoding mode and optimizations, while quality
    /// controls the visual fidelity (0-100, higher = better quality).
    ///
    /// Dimensions are provided later when encoding.
    ///
    /// # Examples
    ///
    /// ```
    /// use mozjpeg::typed::*;
    ///
    /// // Recommended: progressive with optimizations at Q85
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    ///
    /// // Maximum compression (slower)
    /// let config = JpegConfig::from_preset(Preset::ProgressiveSmallest, 75.0);
    ///
    /// // Fastest encoding
    /// let config = JpegConfig::from_preset(Preset::SequentialFastest, 85.0);
    /// ```
    pub fn from_preset(preset: Preset, quality: f32) -> Self {
        Self {
            input: JpegInput::Scanlines(
                ScanlineConfig::RgbToYCbCr {
                    subsampling: ChromaSubsampling::Yuv420,
                    smoothing: 0,
                }
            ),
            compression: preset.compression_mode(),
            qtables: QTableConfig::FromQuality,
            quality,
            optimize_coding: preset.optimize_coding(),
            scan_mode: ScanMode::Auto,
            force_8bit_quantization: false,
        }
    }

    /// Create config for RGB → YCbCr 4:2:0 (most common)
    ///
    /// Convenience constructor for the most common JPEG encoding scenario:
    /// RGB input converted to YCbCr with 4:2:0 chroma subsampling.
    ///
    /// Uses sequential mode with Huffman optimization enabled.
    ///
    /// # Arguments
    ///
    /// * `quality` - Quality parameter (0-100, higher = better quality)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// let config = JpegConfig::rgb_to_ycbcr_420(85.0);
    /// ```
    pub fn rgb_to_ycbcr_420(quality: f32) -> Self {
        Self {
            input: JpegInput::Scanlines(
                ScanlineConfig::RgbToYCbCr {
                    subsampling: ChromaSubsampling::Yuv420,
                    smoothing: 0,
                }
            ),
            compression: CompressionMode::Sequential,
            qtables: QTableConfig::FromQuality,
            quality,
            optimize_coding: true,
            scan_mode: ScanMode::Auto,
            force_8bit_quantization: false,
        }
    }

    /// Create config for RGB → YCbCr 4:4:4 (no chroma subsampling)
    ///
    /// RGB input converted to YCbCr with full-resolution chroma (4:4:4).
    /// Produces larger files but preserves all color detail.
    ///
    /// Uses sequential mode with Huffman optimization enabled.
    ///
    /// # Arguments
    ///
    /// * `quality` - Quality parameter (0-100, higher = better quality)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// // For images with sharp color transitions
    /// let config = JpegConfig::rgb_to_ycbcr_444(90.0);
    /// ```
    pub fn rgb_to_ycbcr_444(quality: f32) -> Self {
        Self {
            input: JpegInput::Scanlines(
                ScanlineConfig::RgbToYCbCr {
                    subsampling: ChromaSubsampling::Yuv444,
                    smoothing: 0,
                }
            ),
            compression: CompressionMode::Sequential,
            qtables: QTableConfig::FromQuality,
            quality,
            optimize_coding: true,
            scan_mode: ScanMode::Auto,
            force_8bit_quantization: false,
        }
    }

    /// Create config for RGB → RGB JPEG (rare, no color conversion)
    ///
    /// Encodes RGB directly without converting to YCbCr. All three components
    /// are stored at full resolution with no chroma subsampling.
    ///
    /// Uses sequential mode with Huffman optimization enabled.
    ///
    /// # Arguments
    ///
    /// * `quality` - Quality parameter (0-100, higher = better quality)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// // For screenshots, diagrams, or synthetic images
    /// let config = JpegConfig::rgb_to_rgb(95.0);
    /// ```
    pub fn rgb_to_rgb(quality: f32) -> Self {
        Self {
            input: JpegInput::Scanlines(ScanlineConfig::RgbToRgb),
            compression: CompressionMode::Sequential,
            qtables: QTableConfig::FromQuality,
            quality,
            optimize_coding: true,
            scan_mode: ScanMode::Auto,
            force_8bit_quantization: false,
        }
    }

    /// Create config for grayscale JPEG
    ///
    /// Single-channel grayscale encoding. Most efficient for black & white images.
    ///
    /// Uses sequential mode with Huffman optimization enabled.
    ///
    /// # Arguments
    ///
    /// * `quality` - Quality parameter (0-100, higher = better quality)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// // For black & white photos or document scans
    /// let config = JpegConfig::grayscale(80.0);
    /// ```
    pub fn grayscale(quality: f32) -> Self {
        Self {
            input: JpegInput::Scanlines(ScanlineConfig::Grayscale),
            compression: CompressionMode::Sequential,
            qtables: QTableConfig::FromQuality,
            quality,
            optimize_coding: true,
            scan_mode: ScanMode::Auto,
            force_8bit_quantization: false,
        }
    }

    /// Validate configuration (non-dimensional checks only)
    ///
    /// Dimension-dependent validation happens at encoder creation time.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate quality range
        if !(0.0..=100.0).contains(&self.quality) {
            return Err(ConfigError::InvalidQuality(self.quality));
        }

        // Warn if smoothing is set but ineffective
        if let JpegInput::Scanlines(ref scanline) = self.input {
            if let Some(smoothing) = scanline.smoothing_factor() {
                if smoothing > 0 {
                    match scanline.subsampling() {
                        Some(ChromaSubsampling::Yuv444) => {
                            eprintln!("Warning: smoothing_factor has no effect with 4:4:4 (no downsampling)");
                        }
                        None => {
                            eprintln!("Warning: smoothing_factor has no effect with RGB or Grayscale");
                        }
                        _ => {} // 4:2:0, 4:2:2 - smoothing is effective
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate configuration with dimensions
    ///
    /// Called internally by encoder creation.
    pub(crate) fn validate_with_dimensions(&self, width: usize, height: usize) -> Result<(), ConfigError> {
        // First validate non-dimensional config
        self.validate()?;

        // Validate dimensions
        if width == 0 || height == 0 {
            return Err(ConfigError::InvalidDimensions {
                width,
                height,
            });
        }

        // Validate raw MCU dimensions if applicable
        if let JpegInput::RawMcu(ref raw) = self.input {
            raw.validate(width, height)?;
        }

        Ok(())
    }

    /// Enable progressive mode with default options
    ///
    /// Switches the configuration to progressive encoding. This produces smaller
    /// files that load with a progressive refinement effect.
    ///
    /// This is a builder method that can be chained with other configuration methods.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// let config = JpegConfig::rgb_to_ycbcr_420(85.0)
    ///     .with_progressive();
    /// ```
    pub fn with_progressive(mut self) -> Self {
        self.compression = CompressionMode::Progressive {
            optimize_scans: false,
            use_scans_in_trellis: false,
        };
        self
    }

    /// Set quality
    ///
    /// Updates the quality parameter. Quality must be in range 0.0-100.0.
    ///
    /// This is a builder method that can be chained with other configuration methods.
    ///
    /// # Arguments
    ///
    /// * `quality` - Quality parameter (0-100, higher = better quality)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::{JpegConfig, Preset};
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 75.0)
    ///     .with_quality(85.0);
    /// ```
    pub fn with_quality(mut self, quality: f32) -> Self {
        self.quality = quality;
        self
    }

    /// Set smoothing factor (only effective for scanline modes with subsampling)
    ///
    /// Applies smoothing to chroma channels during downsampling. Only affects
    /// YCbCr modes with chroma subsampling (4:2:0, 4:2:2). Has no effect on
    /// RGB, Grayscale, or 4:4:4 modes.
    ///
    /// This is a builder method that can be chained with other configuration methods.
    ///
    /// # Arguments
    ///
    /// * `smoothing` - Smoothing factor (0-100, higher = more smoothing)
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::JpegConfig;
    /// let config = JpegConfig::rgb_to_ycbcr_420(85.0)
    ///     .with_smoothing(10);
    /// ```
    pub fn with_smoothing(mut self, smoothing: u8) -> Self {
        if let JpegInput::Scanlines(ref mut scanline) = self.input {
            scanline.set_smoothing(smoothing);
        }
        self
    }

    /// Clamp quantization table values to 1-255 (8-bit range).
    ///
    /// When enabled, all quantization table entries are clamped to the 8-bit range.
    /// This affects both quality-generated tables and custom tables.
    ///
    /// This does NOT disable progressive encoding or change any other encoding
    /// parameter — it only constrains quantization table values.
    ///
    /// Corresponds to the `force_baseline` parameter in libjpeg's
    /// `jpeg_set_quality()` and `jpeg_add_quant_table()`.
    ///
    /// This is a builder method that can be chained with other configuration methods.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::{JpegConfig, Preset};
    /// // Progressive JPEG with 8-bit quantization tables
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0)
    ///     .with_force_8bit_quantization(true);
    /// ```
    pub fn with_force_8bit_quantization(mut self, force: bool) -> Self {
        self.force_8bit_quantization = force;
        self
    }

    /// Create scanline encoder (most common case)
    ///
    /// Returns concrete `ScanlineEncoder` - no matching needed.
    ///
    /// # Errors
    /// Returns `ConfigError::WrongInputMode` if config uses `RawMcu` mode.
    pub fn create_encoder(
        self,
        width: usize,
        height: usize,
    ) -> Result<crate::typed::ScanlineEncoder, ConfigError> {
        use crate::typed::ScanlineEncoder;

        // Validate this is scanline mode
        if !matches!(self.input, JpegInput::Scanlines(_)) {
            return Err(ConfigError::WrongInputMode);
        }

        ScanlineEncoder::new(self, width, height)
    }

    /// Create raw MCU planar encoder (advanced - pre-downsampled components)
    ///
    /// Returns concrete `RawMcuEncoder` - no matching needed.
    ///
    /// # Errors
    /// Returns `ConfigError::WrongInputMode` if config uses `Scanlines` mode.
    pub fn create_mcu_planar_encoder(
        self,
        width: usize,
        height: usize,
    ) -> Result<crate::typed::RawMcuEncoder, ConfigError> {
        use crate::typed::RawMcuEncoder;

        // Validate this is raw MCU mode
        if !matches!(self.input, JpegInput::RawMcu(_)) {
            return Err(ConfigError::WrongInputMode);
        }

        RawMcuEncoder::new(self, width, height)
    }

    /// Convenience: encode RGB pixels to JPEG in one call
    ///
    /// # Example
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> std::io::Result<()> {
    /// let pixels = vec![255u8; 640 * 480 * 3];
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let jpeg = config.encode_rgb(&pixels, 640, 480)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn encode_rgb(
        self,
        pixels: &[u8],
        width: usize,
        height: usize,
    ) -> std::io::Result<Vec<u8>> {
        let encoder = self.create_encoder(width, height)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let mut started = encoder.start(Vec::new())?;
        started.write_scanlines(pixels)?;
        started.finish()
    }

    /// Convenience: encode RGB pixels with stride to JPEG in one call
    ///
    /// Use when pixel data has padding between rows (row stride != width * 3).
    ///
    /// # Arguments
    ///
    /// * `pixels` - RGB pixel data (must contain at least `stride * height` bytes)
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride` - Bytes per row (typically `width * 3` or larger if padded)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> std::io::Result<()> {
    /// // Image data with 4-byte alignment per row
    /// let width = 639; // Not divisible by 4
    /// let stride = ((width * 3 + 3) / 4) * 4; // Round up to multiple of 4
    /// let pixels = vec![255u8; stride * 480];
    ///
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let jpeg = config.encode_rgb_strided(&pixels, width, 480, stride)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn encode_rgb_strided(
        self,
        pixels: &[u8],
        width: usize,
        height: usize,
        stride: usize,
    ) -> std::io::Result<Vec<u8>> {
        let encoder = self.create_encoder(width, height)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let mut started = encoder.start(Vec::new())?;
        started.write_scanlines_strided(pixels, stride)?;
        started.finish()
    }
}

// imgref support (feature-gated)
#[cfg(feature = "image_ref")]
impl JpegConfig {
    /// Create scanline encoder from imgref (dimensions extracted automatically)
    ///
    /// Works with both `ImgRef` and `ImgVec` via `.as_ref()`.
    pub fn create_encoder_from_imgref<Pixel: AsRef<[u8]>>(
        self,
        img: &imgref::ImgRef<Pixel>,
    ) -> Result<crate::typed::ScanlineEncoder, ConfigError> {
        self.create_encoder(img.width(), img.height())
    }

    /// Convenience: encode imgref to JPEG in one call
    ///
    /// Dimensions and stride extracted automatically from imgref.
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "image_ref")] {
    /// # use mozjpeg::typed::*;
    /// # use imgref::ImgVec;
    /// # fn example() -> std::io::Result<()> {
    /// let img = ImgVec::new(vec![[255u8; 3]; 640 * 480], 640, 480);
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let jpeg = config.encode_imgref(&img.as_ref())?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn encode_imgref<Pixel: AsRef<[u8]>>(
        self,
        img: &imgref::ImgRef<Pixel>,
    ) -> std::io::Result<Vec<u8>> {
        let encoder = self.create_encoder_from_imgref(img)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let mut started = encoder.start(Vec::new())?;
        started.write_imgref(img)?;
        started.finish()
    }
}

/// Input mode: scanline vs raw MCU
///
/// Determines how pixel data is provided to the encoder.
///
/// Most users should use `Scanlines` mode, which accepts standard RGB or grayscale
/// pixel data and lets the encoder handle color conversion and chroma subsampling.
///
/// `RawMcu` mode is for advanced users who have already performed color conversion
/// and chroma downsampling, and want to provide pre-processed component planes directly.
#[derive(Clone, Debug)]
pub enum JpegInput {
    /// Scanline mode: provide interleaved pixels, library handles conversion and downsampling
    ///
    /// This is the most common mode. You provide RGB or grayscale scanlines, and the
    /// encoder handles:
    /// - Color space conversion (e.g., RGB → YCbCr)
    /// - Chroma subsampling (e.g., 4:2:0)
    /// - MCU alignment
    ///
    /// **When to use**: Nearly all cases. Use this unless you have a specific reason
    /// to pre-process components yourself.
    Scanlines(ScanlineConfig),

    /// Raw MCU mode: provide pre-downsampled components, library only encodes
    ///
    /// Advanced mode where you provide component planes (Y, Cb, Cr) that have already
    /// been color-converted and downsampled to match the target subsampling ratio.
    ///
    /// **When to use**: Only when:
    /// - You already have YCbCr data from another source
    /// - You need custom downsampling algorithms
    /// - You're implementing specialized encoding pipelines
    ///
    /// **Requirements**:
    /// - Component dimensions must match subsampling ratios exactly
    /// - Image dimensions must be multiples of MCU size
    RawMcu(RawMcuConfig),
}

/// Scanline mode configurations (valid input → JPEG color space combinations)
///
/// Defines the color space conversion path from input pixels to JPEG encoding.
///
/// Each variant specifies both the input color space and the target JPEG color space,
/// ensuring type-safe color space handling.
#[derive(Clone, Debug)]
pub enum ScanlineConfig {
    /// RGB input → YCbCr JPEG (most common)
    ///
    /// The encoder converts RGB pixels to YCbCr color space and applies chroma
    /// subsampling to reduce file size.
    ///
    /// **When to use**: Standard photo/image encoding. This is the default for
    /// most JPEG workflows.
    ///
    /// **Tradeoffs**:
    /// - 4:2:0 subsampling: Smallest files, acceptable quality for photos
    /// - 4:4:4 subsampling: Larger files, preserves color detail
    ///
    /// **Input format**: RGB24 (3 bytes per pixel, interleaved: RGBRGBRGB...)
    RgbToYCbCr {
        /// Chroma subsampling ratio (4:2:0, 4:2:2, or 4:4:4)
        subsampling: ChromaSubsampling,

        /// Smoothing factor (0-100), only effective for Yuv420/Yuv422
        ///
        /// Applies smoothing to chroma channels during downsampling. Higher values
        /// produce smoother color transitions but may blur color details.
        ///
        /// **Recommended**: 0 (no smoothing) for most cases
        smoothing: u8,
    },

    /// RGB input → RGB JPEG (rare, no conversion)
    ///
    /// Encodes RGB directly without color space conversion. All three components
    /// use the same quantization table and are stored at full resolution (no subsampling).
    ///
    /// **When to use**:
    /// - Computer graphics with sharp color edges (screenshots, diagrams)
    /// - When YCbCr conversion artifacts are unacceptable
    /// - Lossless workflows requiring RGB preservation
    ///
    /// **Tradeoffs**:
    /// - Larger file sizes (no chroma subsampling benefit)
    /// - Less efficient compression (RGB correlates less than YCbCr)
    /// - Better color accuracy for synthetic images
    ///
    /// **Input format**: RGB24 (3 bytes per pixel, interleaved: RGBRGBRGB...)
    RgbToRgb,

    /// YCbCr input → YCbCr JPEG (passthrough + downsampling)
    ///
    /// User provides YCbCr pixels that have already been color-converted.
    /// The encoder performs chroma downsampling if needed.
    ///
    /// **When to use**:
    /// - YCbCr data from video codecs
    /// - Custom RGB→YCbCr conversion
    /// - Transcoding from other YCbCr formats
    ///
    /// **Input format**: YCbCr (3 bytes per pixel, interleaved: YCbCrYCbCrYCbCr...)
    YCbCrToYCbCr {
        /// Chroma subsampling ratio
        subsampling: ChromaSubsampling,

        /// Smoothing factor (0-100)
        smoothing: u8,
    },

    /// Grayscale input → Grayscale JPEG
    ///
    /// Single-channel grayscale encoding. Most efficient for black & white images.
    ///
    /// **When to use**:
    /// - Black and white photos
    /// - Document scans
    /// - Any single-channel data
    ///
    /// **Input format**: Y8 (1 byte per pixel)
    Grayscale,

    /// CMYK input → CMYK JPEG (print workflows)
    ///
    /// Four-channel CMYK encoding for print production.
    ///
    /// **When to use**:
    /// - Pre-press workflows
    /// - Print production requiring CMYK color space
    /// - Converting from CMYK sources
    ///
    /// **Note**: Not all JPEG decoders support CMYK. Use with caution.
    ///
    /// **Input format**: CMYK (4 bytes per pixel, interleaved: CMYKCMYKCMYK...)
    CmykToCmyk,
}

impl ScanlineConfig {
    /// Get smoothing factor if applicable
    ///
    /// Returns `Some(smoothing)` for modes that support smoothing (YCbCr modes with
    /// subsampling), or `None` for modes where smoothing doesn't apply.
    pub fn smoothing_factor(&self) -> Option<u8> {
        match self {
            ScanlineConfig::RgbToYCbCr { smoothing, .. } |
            ScanlineConfig::YCbCrToYCbCr { smoothing, .. } => Some(*smoothing),
            _ => None,
        }
    }

    /// Set smoothing factor (no-op for modes without smoothing)
    ///
    /// Updates the smoothing value for YCbCr modes. Has no effect on RGB, Grayscale,
    /// or CMYK modes (silently ignored).
    pub fn set_smoothing(&mut self, value: u8) {
        match self {
            ScanlineConfig::RgbToYCbCr { smoothing, .. } |
            ScanlineConfig::YCbCrToYCbCr { smoothing, .. } => {
                *smoothing = value;
            }
            _ => {} // No smoothing for RGB, Grayscale, CMYK
        }
    }

    /// Get subsampling if applicable
    ///
    /// Returns `Some(subsampling)` for YCbCr modes, or `None` for modes without
    /// chroma subsampling (RGB, Grayscale, CMYK).
    pub fn subsampling(&self) -> Option<ChromaSubsampling> {
        match self {
            ScanlineConfig::RgbToYCbCr { subsampling, .. } |
            ScanlineConfig::YCbCrToYCbCr { subsampling, .. } => Some(*subsampling),
            _ => None,
        }
    }

    /// Get input color space
    ///
    /// Returns the expected format of input pixel data.
    pub fn input_color_space(&self) -> ColorSpace {
        match self {
            ScanlineConfig::RgbToYCbCr { .. } => ColorSpace::JCS_RGB,
            ScanlineConfig::RgbToRgb => ColorSpace::JCS_RGB,
            ScanlineConfig::YCbCrToYCbCr { .. } => ColorSpace::JCS_YCbCr,
            ScanlineConfig::Grayscale => ColorSpace::JCS_GRAYSCALE,
            ScanlineConfig::CmykToCmyk => ColorSpace::JCS_CMYK,
        }
    }

    /// Get JPEG color space
    ///
    /// Returns the color space used in the encoded JPEG file.
    pub fn jpeg_color_space(&self) -> ColorSpace {
        match self {
            ScanlineConfig::RgbToYCbCr { .. } => ColorSpace::JCS_YCbCr,
            ScanlineConfig::RgbToRgb => ColorSpace::JCS_RGB,
            ScanlineConfig::YCbCrToYCbCr { .. } => ColorSpace::JCS_YCbCr,
            ScanlineConfig::Grayscale => ColorSpace::JCS_GRAYSCALE,
            ScanlineConfig::CmykToCmyk => ColorSpace::JCS_CMYK,
        }
    }
}

/// Raw MCU mode configurations (pre-downsampled components)
///
/// Advanced configuration for providing component planes directly to the encoder.
///
/// This mode bypasses the normal scanline encoding path and allows you to provide
/// YCbCr or grayscale component data that has already been color-converted and
/// downsampled to the correct dimensions.
#[derive(Clone, Debug)]
pub enum RawMcuConfig {
    /// Pre-downsampled YCbCr components
    ///
    /// Provide three separate component planes (Y, Cb, Cr) that have already been:
    /// - Color-converted from RGB to YCbCr
    /// - Downsampled according to the specified subsampling ratio
    /// - Aligned to MCU boundaries
    ///
    /// **When to use**:
    /// - Transcoding from video formats that already provide YCbCr planes
    /// - Custom downsampling algorithms
    /// - Specialized image processing pipelines
    ///
    /// **Requirements**:
    /// - Y component must match image dimensions exactly
    /// - Cb/Cr components must match dimensions calculated from subsampling ratio
    /// - All dimensions must be aligned to MCU boundaries
    ///
    /// **Example** (640×480 with 4:2:0):
    /// - Y: 640×480 (full resolution)
    /// - Cb: 320×240 (half width, half height)
    /// - Cr: 320×240 (half width, half height)
    /// - MCU size: 16×16 pixels
    YCbCr {
        /// Chroma subsampling ratio (must match actual component dimensions)
        subsampling: ChromaSubsampling,

        /// Y (luma) component size in pixels (width, height)
        ///
        /// Must equal the image dimensions.
        y_size: (usize, usize),

        /// Cb (chroma blue) component size in pixels (width, height)
        ///
        /// Must match `subsampling.chroma_size(y_size.0, y_size.1)`.
        cb_size: (usize, usize),

        /// Cr (chroma red) component size in pixels (width, height)
        ///
        /// Must match `subsampling.chroma_size(y_size.0, y_size.1)`.
        cr_size: (usize, usize),
    },

    /// Grayscale (single component)
    ///
    /// Provide a single grayscale component plane.
    ///
    /// **Requirements**:
    /// - Dimensions must be multiples of 8 (DCT block size)
    ///
    /// **When to use**:
    /// - Pre-processed grayscale data
    /// - Single-channel scientific/medical imaging
    Grayscale {
        /// Component size in pixels (width, height)
        ///
        /// Both width and height must be multiples of 8.
        size: (usize, usize),
    },
}

impl RawMcuConfig {
    /// Validate MCU dimensions
    ///
    /// Checks that:
    /// - Image dimensions are multiples of MCU size
    /// - Component sizes match the specified subsampling ratios
    /// - All component dimensions are valid
    ///
    /// Returns `ConfigError` if validation fails.
    pub fn validate(&self, width: usize, height: usize) -> Result<(), ConfigError> {
        match self {
            RawMcuConfig::YCbCr { subsampling, y_size, cb_size, cr_size } => {
                let mcu_size = subsampling.mcu_size();

                // Check image dimensions are multiples of MCU size
                if width % mcu_size.0 != 0 || height % mcu_size.1 != 0 {
                    return Err(ConfigError::InvalidMcuDimensions {
                        width,
                        height,
                        mcu_size,
                    });
                }

                // Check Y component size matches image size
                if *y_size != (width, height) {
                    return Err(ConfigError::InvalidComponentSize {
                        component: "Y",
                        expected: (width, height),
                        actual: *y_size,
                    });
                }

                // Check Cb/Cr sizes match subsampling
                let expected_cb_size = subsampling.chroma_size(width, height);
                if *cb_size != expected_cb_size {
                    return Err(ConfigError::InvalidComponentSize {
                        component: "Cb",
                        expected: expected_cb_size,
                        actual: *cb_size,
                    });
                }
                if *cr_size != expected_cb_size {
                    return Err(ConfigError::InvalidComponentSize {
                        component: "Cr",
                        expected: expected_cb_size,
                        actual: *cr_size,
                    });
                }

                Ok(())
            }
            RawMcuConfig::Grayscale { size } => {
                // Grayscale must be multiple of 8 (DCT block size)
                if size.0 % 8 != 0 || size.1 % 8 != 0 {
                    return Err(ConfigError::InvalidMcuDimensions {
                        width: size.0,
                        height: size.1,
                        mcu_size: (8, 8),
                    });
                }
                Ok(())
            }
        }
    }
}

/// Chroma subsampling modes
///
/// Defines how chroma (color) information is downsampled relative to luma (brightness).
///
/// JPEG exploits the human visual system's higher sensitivity to brightness than
/// color by storing chroma at lower resolution. This significantly reduces file size
/// with minimal perceptual quality loss for photographic content.
///
/// The notation "4:X:Y" describes samples in a 4-pixel horizontal region:
/// - First number (4): Luma samples (always 4, meaning full resolution)
/// - Second number (X): Chroma samples in first row
/// - Third number (Y): Chroma samples in second row
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromaSubsampling {
    /// 4:4:4 - No subsampling (full resolution chroma)
    ///
    /// All components stored at full resolution. No color information is discarded.
    ///
    /// **When to use**:
    /// - Images with sharp color transitions (graphics, screenshots, text)
    /// - When maximum color accuracy is required
    /// - Synthetic images where chroma correlates with luma
    ///
    /// **Tradeoffs**:
    /// - Largest file size (typically 20-30% larger than 4:2:0)
    /// - Best color quality
    /// - Minimal compression benefit from chroma subsampling
    ///
    /// **Dimensions**: For 640×480 image:
    /// - Y: 640×480
    /// - Cb: 640×480
    /// - Cr: 640×480
    /// - MCU size: 8×8 pixels
    Yuv444,

    /// 4:2:2 - Horizontal subsampling (half-width chroma)
    ///
    /// Chroma downsampled by 2× horizontally, full resolution vertically.
    ///
    /// **When to use**:
    /// - Video workflows (common in broadcast formats)
    /// - Images with more horizontal than vertical color detail
    /// - When vertical color accuracy is important
    ///
    /// **Tradeoffs**:
    /// - ~50% less chroma data than 4:4:4
    /// - Good for images with horizontal edges
    /// - Less common than 4:2:0 for still images
    ///
    /// **Dimensions**: For 640×480 image:
    /// - Y: 640×480
    /// - Cb: 320×480
    /// - Cr: 320×480
    /// - MCU size: 16×8 pixels
    Yuv422,

    /// 4:2:0 - Both directions (quarter-size chroma)
    ///
    /// Chroma downsampled by 2× both horizontally and vertically. This is the
    /// **most common** subsampling mode for JPEG images.
    ///
    /// **When to use**:
    /// - General photography and web images (default choice)
    /// - Natural scenes where chroma varies slowly
    /// - When file size is important
    ///
    /// **Tradeoffs**:
    /// - 75% less chroma data than 4:4:4 (significant size reduction)
    /// - Minimal perceptual quality loss for photos
    /// - May show color fringing on sharp edges
    /// - Best compression ratio for natural images
    ///
    /// **Dimensions**: For 640×480 image:
    /// - Y: 640×480
    /// - Cb: 320×240
    /// - Cr: 320×240
    /// - MCU size: 16×16 pixels
    Yuv420,

    /// 4:1:1 - Aggressive horizontal (quarter-width chroma)
    ///
    /// Chroma downsampled by 4× horizontally, full resolution vertically.
    /// Rarely used in practice.
    ///
    /// **When to use**:
    /// - Specialized applications requiring strong horizontal compression
    /// - Legacy compatibility with older systems
    ///
    /// **Tradeoffs**:
    /// - 75% less chroma data than 4:4:4
    /// - Visible color artifacts on vertical edges
    /// - Unusual MCU size may complicate processing
    /// - Not recommended for general use (prefer 4:2:0 instead)
    ///
    /// **Dimensions**: For 640×480 image:
    /// - Y: 640×480
    /// - Cb: 160×480
    /// - Cr: 160×480
    /// - MCU size: 32×8 pixels
    Yuv411,
}

impl ChromaSubsampling {
    /// Get component sampling factors as (h, v) for [Y, Cb, Cr]
    ///
    /// Returns horizontal and vertical sampling factors for each component.
    /// Higher values indicate that component is sampled more densely.
    ///
    /// For example, 4:2:0 returns `[(2, 2), (1, 1), (1, 1)]` meaning:
    /// - Y component: 2× samples horizontally and vertically (full resolution)
    /// - Cb/Cr components: 1× samples (half resolution in each direction)
    ///
    /// These factors are used by libjpeg to determine MCU structure.
    pub fn sampling_factors(&self) -> [(u8, u8); 3] {
        match self {
            ChromaSubsampling::Yuv444 => [(1, 1), (1, 1), (1, 1)],
            ChromaSubsampling::Yuv422 => [(2, 1), (1, 1), (1, 1)],
            ChromaSubsampling::Yuv420 => [(2, 2), (1, 1), (1, 1)],
            ChromaSubsampling::Yuv411 => [(4, 1), (1, 1), (1, 1)],
        }
    }

    /// Get MCU (Minimum Coded Unit) size in pixels
    ///
    /// Returns the dimensions of the minimum coded unit for this subsampling mode.
    /// Image dimensions must be multiples of this size when using raw MCU mode.
    ///
    /// MCU size is calculated as: `(max_h_factor * 8, max_v_factor * 8)`
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::ChromaSubsampling;
    /// assert_eq!(ChromaSubsampling::Yuv420.mcu_size(), (16, 16));
    /// assert_eq!(ChromaSubsampling::Yuv422.mcu_size(), (16, 8));
    /// assert_eq!(ChromaSubsampling::Yuv444.mcu_size(), (8, 8));
    /// ```
    pub fn mcu_size(&self) -> (usize, usize) {
        let factors = self.sampling_factors();
        let max_h = factors.iter().map(|(h, _)| h).max().unwrap();
        let max_v = factors.iter().map(|(_, v)| v).max().unwrap();
        ((*max_h as usize) * 8, (*max_v as usize) * 8)
    }

    /// Calculate chroma component size given luma size
    ///
    /// Given full-resolution luma (Y) dimensions, returns the corresponding
    /// chroma (Cb/Cr) dimensions based on the subsampling ratio.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mozjpeg::typed::ChromaSubsampling;
    /// // 4:2:0 - both dimensions halved
    /// assert_eq!(ChromaSubsampling::Yuv420.chroma_size(640, 480), (320, 240));
    ///
    /// // 4:2:2 - only width halved
    /// assert_eq!(ChromaSubsampling::Yuv422.chroma_size(640, 480), (320, 480));
    ///
    /// // 4:4:4 - no change
    /// assert_eq!(ChromaSubsampling::Yuv444.chroma_size(640, 480), (640, 480));
    /// ```
    pub fn chroma_size(&self, luma_width: usize, luma_height: usize) -> (usize, usize) {
        match self {
            ChromaSubsampling::Yuv444 => (luma_width, luma_height),
            ChromaSubsampling::Yuv422 => (luma_width / 2, luma_height),
            ChromaSubsampling::Yuv420 => (luma_width / 2, luma_height / 2),
            ChromaSubsampling::Yuv411 => (luma_width / 4, luma_height),
        }
    }
}

/// Compression mode (sequential vs progressive)
///
/// Determines how the JPEG image is encoded and decoded.
#[derive(Clone, Debug)]
pub enum CompressionMode {
    /// Sequential JPEG
    ///
    /// Standard JPEG encoding where the image is stored as a single scan from
    /// top to bottom. The decoder must process the entire file sequentially.
    ///
    /// **When to use**:
    /// - Maximum decoder compatibility
    /// - Fastest encoding (no progressive scan optimization)
    /// - Applications requiring sequential access (video players, embedded systems)
    ///
    /// **Tradeoffs**:
    /// - Slightly larger file sizes (~2-5% larger than progressive)
    /// - Image appears incrementally from top to bottom during loading
    /// - Simpler encoding and decoding
    Sequential,

    /// Progressive JPEG (with scan optimization options)
    ///
    /// Image is encoded in multiple scans, allowing the decoder to display a
    /// low-quality preview that progressively refines as more data arrives.
    ///
    /// **When to use**:
    /// - Web images (better perceived loading experience)
    /// - Smaller file sizes with same quality
    /// - When decoder supports progressive mode (most modern decoders do)
    ///
    /// **Tradeoffs**:
    /// - Slightly smaller files than sequential (~2-5% smaller)
    /// - Slower encoding (especially with `optimize_scans`)
    /// - Better perceived loading (low-res preview appears first)
    /// - Requires more decoder memory
    ///
    /// **Encoding time**: Moderate to slow depending on optimization level
    Progressive {
        /// Optimize progressive scan script (MozJPEG specific)
        ///
        /// When `true`, tries multiple progressive scan configurations to find
        /// the one that produces the smallest output. This adds ~100% encoding
        /// time for only ~1% additional size reduction.
        ///
        /// **Recommended**: `false` for most cases (use `Preset::ProgressiveBalanced`)
        /// **Use `true` for**: Offline encoding where file size is critical
        optimize_scans: bool,

        /// Use scans in trellis quantization (MozJPEG specific)
        ///
        /// When `true`, the trellis quantizer considers the progressive scan
        /// structure during optimization. This can improve quality slightly but
        /// requires `optimize_scans = true` to be effective.
        ///
        /// **Recommended**: `true` when `optimize_scans = true`, `false` otherwise
        use_scans_in_trellis: bool,
    },
}

impl CompressionMode {
    /// Check if progressive
    ///
    /// Returns `true` if this is progressive mode, `false` for sequential.
    pub fn is_progressive(&self) -> bool {
        matches!(self, CompressionMode::Progressive { .. })
    }
}

/// Quantization table configuration
///
/// Controls how quantization tables are generated or selected. Quantization tables
/// determine how DCT coefficients are quantized, directly affecting both image
/// quality and file size.
///
/// Most users should use `FromQuality`, which generates standard JPEG tables based
/// on the quality parameter. Advanced users can provide custom tables for specialized
/// encoding needs.
#[derive(Clone, Debug)]
pub enum QTableConfig {
    /// Use quality parameter to generate standard JPEG Annex K tables
    ///
    /// The quality parameter (0-100) is mapped to quantization tables using the
    /// standard JPEG Annex K algorithm. This is the recommended default.
    ///
    /// **When to use**: Nearly all cases. This produces standard-compliant JPEG files
    /// with predictable quality/size tradeoffs.
    ///
    /// - Quality 75-95: Recommended for photos (85 is a good default)
    /// - Quality 95-100: Near-lossless (large files, minimal artifacts)
    /// - Quality 50-75: Smaller files with visible artifacts
    /// - Quality 0-50: Heavy compression (significant artifacts)
    FromQuality,

    /// Explicit table assignment
    ///
    /// Provide custom quantization tables for fine-grained control over compression.
    ///
    /// **When to use**:
    /// - Specialized encoding (medical imaging, archival, etc.)
    /// - Research and experimentation with quantization
    /// - Reproducing specific encoder behavior
    ///
    /// **Note**: Custom tables should be carefully designed. Poor tables can produce
    /// worse quality than standard tables at the same file size.
    Explicit {
        /// Luma table (for Y in YCbCr, or all RGB components, or grayscale)
        ///
        /// This table is used for:
        /// - Y component in YCbCr mode
        /// - All three components in RGB mode
        /// - Single component in grayscale mode
        luma: QTableChoice,

        /// Chroma table (for Cb/Cr in YCbCr)
        ///
        /// Only used in YCbCr mode. Must be `None` for RGB and grayscale.
        ///
        /// If `None` in YCbCr mode, the luma table will be used for all components.
        chroma: Option<QTableChoice>,
    },
}

/// How to select/provide a quantization table
///
/// Used within `QTableConfig::Explicit` to specify individual tables.
#[derive(Clone, Debug)]
pub enum QTableChoice {
    /// Use quality parameter to generate tables
    ///
    /// Uses the standard JPEG Annex K algorithm to generate a table based on
    /// the quality value.
    FromQuality,

    /// Use a custom quantization table
    ///
    /// Provide an 8×8 table of quantization values. Values typically range from
    /// 1 to 255, with lower values preserving more detail (larger files) and
    /// higher values discarding more detail (smaller files).
    ///
    /// **Example**: Preserve low-frequency detail, discard high-frequency:
    /// ```text
    /// [  2,  2,  2,  4,  8, 16, 32, 64,
    ///    2,  2,  2,  4,  8, 16, 32, 64,
    ///    2,  2,  4,  8, 16, 32, 64, 64,
    ///    4,  4,  8, 16, 32, 64, 64, 64,
    ///    8,  8, 16, 32, 64, 64, 64, 64,
    ///   16, 16, 32, 64, 64, 64, 64, 64,
    ///   32, 32, 64, 64, 64, 64, 64, 64,
    ///   64, 64, 64, 64, 64, 64, 64, 64 ]
    /// ```
    Custom(Box<QTable>),

    // Future: add presets (MozJPEG, Klein, Watson, etc.)
    // Preset(QTablePreset),
}

/// Configuration errors
///
/// Errors that can occur during configuration validation or encoder creation.
#[derive(Debug)]
#[non_exhaustive]
pub enum ConfigError {
    /// Invalid image dimensions (zero width or height)
    ///
    /// Both width and height must be greater than zero.
    InvalidDimensions {
        /// Image width in pixels
        width: usize,
        /// Image height in pixels
        height: usize,
    },

    /// Invalid quality parameter
    ///
    /// Quality must be in the range 0.0 to 100.0 (inclusive).
    ///
    /// - 0.0: Maximum compression (lowest quality)
    /// - 100.0: Minimum compression (highest quality)
    InvalidQuality(f32),

    /// Invalid MCU dimensions for raw MCU mode
    ///
    /// In raw MCU mode, image dimensions must be multiples of the MCU size,
    /// which depends on the chroma subsampling mode:
    /// - 4:4:4: 8×8 pixels
    /// - 4:2:2: 16×8 pixels
    /// - 4:2:0: 16×16 pixels
    /// - 4:1:1: 32×8 pixels
    InvalidMcuDimensions {
        /// Image width in pixels
        width: usize,
        /// Image height in pixels
        height: usize,
        /// Required MCU size (width, height) in pixels
        mcu_size: (usize, usize),
    },

    /// Invalid component size in raw MCU mode
    ///
    /// Component planes must have dimensions that match the subsampling ratio.
    /// For example, with 4:2:0 subsampling and 640×480 image:
    /// - Y component must be 640×480
    /// - Cb component must be 320×240
    /// - Cr component must be 320×240
    InvalidComponentSize {
        /// Component name ("Y", "Cb", or "Cr")
        component: &'static str,
        /// Expected dimensions (width, height) in pixels
        expected: (usize, usize),
        /// Actual dimensions (width, height) in pixels
        actual: (usize, usize),
    },

    /// Wrong input mode for this encoder type
    ///
    /// This error occurs when:
    /// - Calling `create_encoder()` with `RawMcu` config (use `create_mcu_planar_encoder()`)
    /// - Calling `create_mcu_planar_encoder()` with `Scanlines` config (use `create_encoder()`)
    WrongInputMode,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidDimensions { width, height } => {
                write!(f, "Invalid dimensions: {}×{} (must be > 0)", width, height)
            }
            ConfigError::InvalidQuality(q) => {
                write!(f, "Invalid quality: {} (must be 0-100)", q)
            }
            ConfigError::InvalidMcuDimensions { width, height, mcu_size } => {
                write!(f, "Invalid MCU dimensions: {}×{} must be multiples of {}×{}",
                       width, height, mcu_size.0, mcu_size.1)
            }
            ConfigError::InvalidComponentSize { component, expected, actual } => {
                write!(f, "Invalid {} component size: expected {}×{}, got {}×{}",
                       component, expected.0, expected.1, actual.0, actual.1)
            }
            ConfigError::WrongInputMode => {
                write!(f, "Wrong input mode for this encoder type")
            }
        }
    }
}

impl std::error::Error for ConfigError {}
