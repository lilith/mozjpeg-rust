//! JPEG encoder implementation for type-safe configuration
//!
//! This module provides two encoder types:
//! - [`ScanlineEncoder`]: Standard encoding from RGB/grayscale pixels (most common)
//! - [`RawMcuEncoder`]: Advanced encoding from pre-downsampled YCbCr components
//!
//! See the parent [`typed`](super) module for usage examples.

use std::io;
use std::marker::PhantomData;

use crate::{Compress, ColorSpace};
use crate::compress::CompressStarted;
use super::*;

/// Scanline mode encoder for standard JPEG encoding.
///
/// This encoder accepts interleaved RGB or grayscale pixel data and handles all
/// color conversion, chroma subsampling, and MCU alignment automatically. This is
/// the **recommended encoder for most use cases**.
///
/// # What is Scanline Mode?
///
/// In scanline mode, you provide pixel data row-by-row in a standard interleaved
/// format (e.g., RGBRGBRGB...). The encoder handles:
/// - Color space conversion (RGB → YCbCr, if configured)
/// - Chroma subsampling (4:2:0, 4:2:2, etc.)
/// - MCU alignment and padding
/// - DCT, quantization, and Huffman coding
///
/// # When to Use This Encoder
///
/// Use `ScanlineEncoder` when:
/// - You have RGB or grayscale pixel data
/// - You want the library to handle color conversion and downsampling
/// - You're encoding standard photos, screenshots, or synthetic images
///
/// For advanced use cases where you've already performed color conversion and
/// downsampling, see [`RawMcuEncoder`].
///
/// # Workflow
///
/// 1. Create a [`JpegConfig`] with your desired settings
/// 2. Call [`config.create_encoder(width, height)`](JpegConfig::create_encoder)
/// 3. Start compression with [`start(writer)`](ScanlineEncoder::start)
/// 4. Write pixel data with [`write_scanlines()`](ScanlineEncoderStarted::write_scanlines)
/// 5. Finish encoding with [`finish()`](ScanlineEncoderStarted::finish)
///
/// # Example
///
/// ```no_run
/// use mozjpeg::typed::*;
/// use std::fs::File;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create RGB pixel data (640×480, 3 bytes per pixel)
/// let pixels = vec![255u8; 640 * 480 * 3];
///
/// // Configure encoder
/// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
/// let encoder = config.create_encoder(640, 480)?;
///
/// // Encode to file
/// let file = File::create("output.jpg")?;
/// let mut started = encoder.start(file)?;
/// started.write_scanlines(&pixels)?;
/// started.finish()?;
/// # Ok(())
/// # }
/// ```
///
/// # Supported Input Formats
///
/// The input format depends on the [`ScanlineConfig`] variant:
/// - [`RgbToYCbCr`](ScanlineConfig::RgbToYCbCr): RGB24 (3 bytes/pixel: RGBRGBRGB...)
/// - [`RgbToRgb`](ScanlineConfig::RgbToRgb): RGB24 (3 bytes/pixel: RGBRGBRGB...)
/// - [`YCbCrToYCbCr`](ScanlineConfig::YCbCrToYCbCr): YCbCr (3 bytes/pixel: YCbCrYCbCrYCbCr...)
/// - [`Grayscale`](ScanlineConfig::Grayscale): Y8 (1 byte/pixel)
/// - [`CmykToCmyk`](ScanlineConfig::CmykToCmyk): CMYK (4 bytes/pixel: CMYKCMYKCMYK...)
///
/// # See Also
///
/// - [`RawMcuEncoder`] for pre-downsampled component data
/// - [`JpegConfig`] for configuration options
/// - [`ScanlineConfig`] for input format details
pub struct ScanlineEncoder {
    compress: Compress,
    config: JpegConfig,
}

impl ScanlineEncoder {
    pub(crate) fn new(config: JpegConfig, width: usize, height: usize) -> Result<Self, ConfigError> {
        // Validate dimensions
        if width == 0 || height == 0 {
            return Err(ConfigError::InvalidDimensions { width, height });
        }

        // Extract scanline config
        let scanline = match &config.input {
            JpegInput::Scanlines(sc) => sc,
            _ => return Err(ConfigError::WrongInputMode),
        };

        // Create compress instance
        let mut compress = Compress::new(scanline.input_color_space());

        // Set dimensions
        compress.set_size(width, height);

        // Set JPEG color space (may differ from input)
        compress.set_color_space(scanline.jpeg_color_space());

        // Call set_scan_optimization_mode FIRST — it calls jpeg_set_defaults()
        // which resets quality, smoothing, progressive mode, subsampling, and
        // other settings. Everything else must come after.
        compress.set_scan_optimization_mode(config.scan_mode);

        // Now set everything that jpeg_set_defaults() would have reset:

        // Quality and quantization tables (respecting force_8bit_quantization)
        match &config.qtables {
            QTableConfig::FromQuality => {
                compress.set_quality_force_8bit(config.quality, config.force_8bit_quantization);
            }
            QTableConfig::Explicit { luma, chroma } => {
                match luma {
                    QTableChoice::FromQuality => {
                        compress.set_quality_force_8bit(config.quality, config.force_8bit_quantization);
                    }
                    QTableChoice::Custom(qtable) => {
                        compress.set_luma_qtable_force_8bit(qtable, config.force_8bit_quantization);
                    }
                }

                if let Some(chroma_choice) = chroma {
                    match chroma_choice {
                        QTableChoice::FromQuality => {
                            // Already set by quality
                        }
                        QTableChoice::Custom(qtable) => {
                            compress.set_chroma_qtable_force_8bit(qtable, config.force_8bit_quantization);
                        }
                    }
                }
            }
        }

        // Huffman optimization
        compress.set_optimize_coding(config.optimize_coding);

        // Smoothing (reset to 0 by jpeg_set_defaults)
        if let Some(smoothing) = scanline.smoothing_factor() {
            compress.set_smoothing_factor(smoothing);
        }

        // Progressive mode and scan options
        match &config.compression {
            CompressionMode::Sequential => {}
            CompressionMode::Progressive { optimize_scans, use_scans_in_trellis } => {
                compress.set_progressive_mode();

                if *optimize_scans {
                    compress.set_optimize_scans(true);
                }

                if *use_scans_in_trellis {
                    compress.set_use_scans_in_trellis(true);
                }
            }
        }

        // Subsampling factors (reset to colorspace defaults by jpeg_set_defaults)
        if let Some(subsampling) = scanline.subsampling() {
            let factors = subsampling.sampling_factors();
            let comps = compress.components_mut();
            for (i, (h, v)) in factors.iter().enumerate() {
                if i < comps.len() {
                    comps[i].h_samp_factor = *h as i32;
                    comps[i].v_samp_factor = *v as i32;
                }
            }
        }

        Ok(Self { compress, config })
    }

    /// Start compression and return a started encoder.
    ///
    /// Begins the JPEG encoding process and prepares the encoder to accept pixel data.
    /// The JPEG header is written to the provided writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - Output destination for the JPEG data (file, buffer, etc.)
    ///
    /// # Returns
    ///
    /// A [`ScanlineEncoderStarted`] instance ready to accept pixel data via
    /// [`write_scanlines()`](ScanlineEncoderStarted::write_scanlines) or related methods.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing the JPEG header fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(640, 480)?;
    ///
    /// // Start compression to a Vec<u8> buffer
    /// let mut started = encoder.start(Vec::new())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn start<W: io::Write>(self, writer: W) -> io::Result<ScanlineEncoderStarted<W>> {
        let started = self.compress.start_compress(writer)?;
        Ok(ScanlineEncoderStarted {
            compress: started,
            config: self.config,
        })
    }
}

/// Started scanline encoder ready to accept pixel data.
///
/// This type represents a JPEG encoder that has been started and is ready to receive
/// pixel data. The encoder has already written the JPEG header and is waiting for
/// image scanlines.
///
/// # Writing Pixel Data
///
/// The encoder provides methods for writing pixel data:
/// - [`write_scanlines()`](Self::write_scanlines): Tightly-packed pixel data (most common)
/// - [`write_scanlines_strided()`](Self::write_scanlines_strided): Pixel data with row padding
///
/// When the `image_ref` feature is enabled, an additional method is available:
/// - `write_imgref()`: From `imgref::ImgRef` or `imgref::ImgVec`
///
/// # Finishing Encoding
///
/// Call [`finish()`](Self::finish) to complete encoding and retrieve the output writer.
/// Failing to call `finish()` will result in an incomplete JPEG file.
///
/// # Example
///
/// ```no_run
/// # use mozjpeg::typed::*;
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let pixels = vec![255u8; 640 * 480 * 3];
/// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
/// let encoder = config.create_encoder(640, 480)?;
///
/// let mut started = encoder.start(Vec::new())?;
/// started.write_scanlines(&pixels)?;
/// let jpeg_data = started.finish()?;
/// # Ok(())
/// # }
/// ```
pub struct ScanlineEncoderStarted<W> {
    compress: CompressStarted<W>,
    config: JpegConfig,
}

impl<W: io::Write> ScanlineEncoderStarted<W> {
    /// Write tightly-packed pixel data to the encoder.
    ///
    /// Writes interleaved pixel data with no padding between rows. The data must contain
    /// exactly `width * height * bytes_per_pixel` bytes, where `bytes_per_pixel` depends
    /// on the configured input color space:
    /// - RGB: 3 bytes per pixel
    /// - Grayscale: 1 byte per pixel
    /// - CMYK: 4 bytes per pixel
    ///
    /// # Arguments
    ///
    /// * `data` - Pixel data buffer (tightly packed, row-major order)
    ///
    /// # Errors
    ///
    /// Returns an I/O error if:
    /// - Writing to the output fails
    /// - Data buffer is too small
    /// - Internal compression error occurs
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create tightly-packed RGB data (no padding)
    /// let width = 640;
    /// let height = 480;
    /// let pixels = vec![255u8; width * height * 3];
    ///
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(width, height)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// // Write all pixels at once
    /// started.write_scanlines(&pixels)?;
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_scanlines(&mut self, data: &[u8]) -> io::Result<()> {
        self.compress.write_scanlines(data)
    }

    /// Write pixel data with custom row stride (padding between rows).
    ///
    /// Use this when your pixel data has padding or alignment bytes at the end of each row.
    /// The `stride` parameter specifies the total number of bytes from the start of one
    /// row to the start of the next row.
    ///
    /// # Arguments
    ///
    /// * `data` - Pixel data buffer (may contain padding at end of rows)
    /// * `stride` - Number of bytes from start of one row to start of next
    ///
    /// # When to Use
    ///
    /// Use `write_scanlines_strided()` when:
    /// - Image data has row alignment requirements (e.g., 4-byte aligned rows)
    /// - Working with buffers from graphics APIs (OpenGL, DirectX, etc.)
    /// - Data comes from external libraries with padding
    ///
    /// For tightly-packed data with no padding, use [`write_scanlines()`](Self::write_scanlines) instead.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if:
    /// - Writing to the output fails
    /// - Stride is smaller than required row size
    /// - Data buffer is too small (`data.len()` must be at least `stride * height`)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let width = 639; // Not a multiple of 4
    /// let height = 480;
    ///
    /// // Calculate aligned stride (round up to multiple of 4 bytes)
    /// let min_stride = width * 3; // RGB
    /// let stride = ((min_stride + 3) / 4) * 4; // Round up to 4-byte alignment
    ///
    /// // Allocate buffer with padding
    /// let pixels = vec![255u8; stride * height];
    ///
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(width, height)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// // Write with explicit stride
    /// started.write_scanlines_strided(&pixels, stride)?;
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_scanlines_strided(&mut self, data: &[u8], stride: usize) -> io::Result<()> {
        self.compress.write_scanlines_strided(data, stride)
    }

    /// Finish compression and return the output writer.
    ///
    /// Completes the JPEG encoding process by:
    /// 1. Flushing any remaining compressed data
    /// 2. Writing the JPEG end-of-image marker
    /// 3. Finalizing the output stream
    /// 4. Returning the underlying writer
    ///
    /// **Important**: You must call this method to produce a valid JPEG file.
    /// Dropping the encoder without calling `finish()` will result in an incomplete
    /// (and likely invalid) JPEG.
    ///
    /// # Returns
    ///
    /// The output writer that was provided to [`start()`](ScanlineEncoder::start).
    /// For `Vec<u8>` writers, this contains the complete JPEG data.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if finalizing the output fails (e.g., disk full, write error).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pixels = vec![255u8; 640 * 480 * 3];
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(640, 480)?;
    ///
    /// let mut started = encoder.start(Vec::new())?;
    /// started.write_scanlines(&pixels)?;
    ///
    /// // Finish encoding and get JPEG data
    /// let jpeg_bytes: Vec<u8> = started.finish()?;
    /// println!("Encoded {} bytes", jpeg_bytes.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn finish(self) -> io::Result<W> {
        self.compress.finish()
    }

    /// Get a reference to the encoder configuration.
    ///
    /// Returns the [`JpegConfig`] that was used to create this encoder. Useful for
    /// inspecting settings after encoding has started.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(640, 480)?;
    /// let started = encoder.start(Vec::new())?;
    ///
    /// // Access config during encoding
    /// println!("Quality: {}", started.config().quality);
    /// # Ok(())
    /// # }
    /// ```
    pub fn config(&self) -> &JpegConfig {
        &self.config
    }
}

/// Raw MCU mode encoder for advanced encoding scenarios.
///
/// This encoder accepts pre-downsampled YCbCr or grayscale component planes and bypasses
/// the normal scanline processing. The library performs only DCT transformation, quantization,
/// and entropy coding.
///
/// # What is Raw MCU Mode?
///
/// Raw MCU (Minimum Coded Unit) mode is an **advanced encoding mode** where you provide
/// component data that has already been:
/// - Color-converted (e.g., RGB → YCbCr)
/// - Chroma-downsampled to match the target subsampling ratio
/// - Aligned to MCU boundaries
///
/// The encoder skips color conversion and downsampling, encoding the component data directly.
///
/// # When to Use This Encoder
///
/// Use `RawMcuEncoder` only when:
/// - You already have YCbCr component planes from another source (video codecs, etc.)
/// - You need custom downsampling algorithms not provided by the library
/// - You're implementing specialized encoding pipelines
/// - Performance is critical and you can amortize conversion costs
///
/// **For most use cases, [`ScanlineEncoder`] is simpler and recommended.**
///
/// # Requirements
///
/// When using raw MCU mode, you must ensure:
/// 1. **Component dimensions** match the subsampling ratio exactly
/// 2. **Image dimensions** are multiples of the MCU size
/// 3. **Component data** is provided as separate planes (not interleaved)
///
/// For example, with 4:2:0 subsampling and 640×480 image:
/// - Y component: 640×480 (full resolution)
/// - Cb component: 320×240 (half width, half height)
/// - Cr component: 320×240 (half width, half height)
/// - MCU size: 16×16 pixels
/// - Image dimensions must be multiples of 16
///
/// # Performance Implications
///
/// **Potential speedup**: Skipping color conversion and downsampling can save ~10-20% encoding time.
///
/// **Tradeoffs**:
/// - You must perform color conversion yourself
/// - You must handle chroma downsampling correctly
/// - Dimension validation is stricter (must align to MCU boundaries)
/// - More complex to use correctly
///
/// # Workflow
///
/// 1. Create a [`JpegConfig`] with [`JpegInput::RawMcu`] configuration
/// 2. Call [`config.create_mcu_planar_encoder(width, height)`](JpegConfig::create_mcu_planar_encoder)
/// 3. Start compression with [`start(writer)`](RawMcuEncoder::start)
/// 4. Write component data with [`write_raw_data()`](RawMcuEncoderStarted::write_raw_data)
/// 5. Finish encoding with [`finish()`](RawMcuEncoderStarted::finish)
///
/// # Example
///
/// ```no_run
/// use mozjpeg::typed::*;
/// use std::fs::File;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Dimensions must be multiples of MCU size (16×16 for 4:2:0)
/// let width = 640;  // Multiple of 16
/// let height = 480; // Multiple of 16
///
/// // Pre-downsampled YCbCr 4:2:0 components
/// let y_plane = vec![128u8; 640 * 480];      // Full resolution
/// let cb_plane = vec![128u8; 320 * 240];     // Half resolution
/// let cr_plane = vec![128u8; 320 * 240];     // Half resolution
///
/// // Configure raw MCU encoder
/// let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
/// config.input = JpegInput::RawMcu(RawMcuConfig::YCbCr {
///     subsampling: ChromaSubsampling::Yuv420,
///     y_size: (640, 480),
///     cb_size: (320, 240),
///     cr_size: (320, 240),
/// });
///
/// let encoder = config.create_mcu_planar_encoder(width, height)?;
///
/// // Encode to file
/// let file = File::create("output.jpg")?;
/// let mut started = encoder.start(file)?;
///
/// // Write component planes
/// started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane])?;
/// started.finish()?;
/// # Ok(())
/// # }
/// ```
///
/// # See Also
///
/// - [`ScanlineEncoder`] for standard RGB/grayscale encoding
/// - [`RawMcuConfig`] for configuration details
/// - [`ChromaSubsampling`] for subsampling modes and MCU sizes
pub struct RawMcuEncoder {
    compress: Compress,
    config: JpegConfig,
}

impl RawMcuEncoder {
    pub(crate) fn new(config: JpegConfig, width: usize, height: usize) -> Result<Self, ConfigError> {
        // Validate dimensions
        config.validate_with_dimensions(width, height)?;

        // Extract raw MCU config
        let raw = match &config.input {
            JpegInput::RawMcu(rc) => rc,
            _ => return Err(ConfigError::WrongInputMode),
        };

        // Determine color space from raw config
        let color_space = match raw {
            RawMcuConfig::YCbCr { .. } => ColorSpace::JCS_YCbCr,
            RawMcuConfig::Grayscale { .. } => ColorSpace::JCS_GRAYSCALE,
        };

        // Create compress instance
        let mut compress = Compress::new(color_space);

        // Set dimensions
        compress.set_size(width, height);

        // Call set_scan_optimization_mode FIRST — it calls jpeg_set_defaults()
        // which resets quality, progressive mode, subsampling, and other settings.
        compress.set_scan_optimization_mode(config.scan_mode);

        // Now set everything that jpeg_set_defaults() would have reset:

        // Quality and quantization tables (respecting force_8bit_quantization)
        match &config.qtables {
            QTableConfig::FromQuality => {
                compress.set_quality_force_8bit(config.quality, config.force_8bit_quantization);
            }
            QTableConfig::Explicit { luma, chroma } => {
                match luma {
                    QTableChoice::FromQuality => {
                        compress.set_quality_force_8bit(config.quality, config.force_8bit_quantization);
                    }
                    QTableChoice::Custom(qtable) => {
                        compress.set_luma_qtable_force_8bit(qtable, config.force_8bit_quantization);
                    }
                }

                if let Some(chroma_choice) = chroma {
                    match chroma_choice {
                        QTableChoice::FromQuality => {}
                        QTableChoice::Custom(qtable) => {
                            compress.set_chroma_qtable_force_8bit(qtable, config.force_8bit_quantization);
                        }
                    }
                }
            }
        }

        // Huffman optimization
        compress.set_optimize_coding(config.optimize_coding);

        // Progressive mode and scan options
        match &config.compression {
            CompressionMode::Sequential => {}
            CompressionMode::Progressive { optimize_scans, use_scans_in_trellis } => {
                compress.set_progressive_mode();

                if *optimize_scans {
                    compress.set_optimize_scans(true);
                }

                if *use_scans_in_trellis {
                    compress.set_use_scans_in_trellis(true);
                }
            }
        }

        // Subsampling factors (reset to colorspace defaults by jpeg_set_defaults)
        match raw {
            RawMcuConfig::YCbCr { subsampling, .. } => {
                let factors = subsampling.sampling_factors();
                let comps = compress.components_mut();
                for (i, (h, v)) in factors.iter().enumerate() {
                    if i < comps.len() {
                        comps[i].h_samp_factor = *h as i32;
                        comps[i].v_samp_factor = *v as i32;
                    }
                }
            }
            RawMcuConfig::Grayscale { .. } => {
                // Single component, no subsampling
            }
        }

        // Enable raw data mode AFTER set_scan_optimization_mode
        // (set_scan_optimization_mode calls jpeg_set_defaults which resets this flag)
        compress.set_raw_data_in(true);

        Ok(Self { compress, config })
    }

    /// Start compression and return a started encoder.
    ///
    /// Begins the JPEG encoding process and prepares the encoder to accept component data.
    /// The JPEG header is written to the provided writer.
    ///
    /// # Arguments
    ///
    /// * `writer` - Output destination for the JPEG data (file, buffer, etc.)
    ///
    /// # Returns
    ///
    /// A [`RawMcuEncoderStarted`] instance ready to accept component planes via
    /// [`write_raw_data()`](RawMcuEncoderStarted::write_raw_data).
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing the JPEG header fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// config.input = JpegInput::RawMcu(RawMcuConfig::YCbCr {
    ///     subsampling: ChromaSubsampling::Yuv420,
    ///     y_size: (640, 480),
    ///     cb_size: (320, 240),
    ///     cr_size: (320, 240),
    /// });
    ///
    /// let encoder = config.create_mcu_planar_encoder(640, 480)?;
    /// let mut started = encoder.start(Vec::new())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn start<W: io::Write>(self, writer: W) -> io::Result<RawMcuEncoderStarted<W>> {
        let started = self.compress.start_compress(writer)?;
        Ok(RawMcuEncoderStarted {
            compress: started,
            config: self.config,
        })
    }
}

/// Started raw MCU encoder ready to accept component data.
///
/// This type represents a raw MCU encoder that has been started and is ready to receive
/// pre-downsampled component planes. The encoder has already written the JPEG header and
/// is waiting for Y, Cb, Cr (or grayscale) component data.
///
/// # Writing Component Data
///
/// Use [`write_raw_data()`](Self::write_raw_data) to provide component planes:
/// - **YCbCr mode**: Pass a slice of three slices: `&[&y_data, &cb_data, &cr_data]`
/// - **Grayscale mode**: Pass a slice of one slice: `&[&y_data]`
///
/// # Component Requirements
///
/// Each component must have the correct dimensions as specified in [`RawMcuConfig`]:
/// - For YCbCr 4:2:0 (640×480): Y=307200 bytes, Cb=76800 bytes, Cr=76800 bytes
/// - For YCbCr 4:2:2 (640×480): Y=307200 bytes, Cb=153600 bytes, Cr=153600 bytes
/// - For YCbCr 4:4:4 (640×480): Y=307200 bytes, Cb=307200 bytes, Cr=307200 bytes
/// - For Grayscale (640×480): Y=307200 bytes
///
/// # Finishing Encoding
///
/// Call [`finish()`](Self::finish) to complete encoding and retrieve the output writer.
/// Failing to call `finish()` will result in an incomplete JPEG file.
///
/// # Example
///
/// ```no_run
/// # use mozjpeg::typed::*;
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let y_plane = vec![128u8; 640 * 480];
/// let cb_plane = vec![128u8; 320 * 240];
/// let cr_plane = vec![128u8; 320 * 240];
///
/// let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
/// config.input = JpegInput::RawMcu(RawMcuConfig::YCbCr {
///     subsampling: ChromaSubsampling::Yuv420,
///     y_size: (640, 480),
///     cb_size: (320, 240),
///     cr_size: (320, 240),
/// });
///
/// let encoder = config.create_mcu_planar_encoder(640, 480)?;
/// let mut started = encoder.start(Vec::new())?;
/// started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane])?;
/// let jpeg_data = started.finish()?;
/// # Ok(())
/// # }
/// ```
pub struct RawMcuEncoderStarted<W> {
    compress: CompressStarted<W>,
    config: JpegConfig,
}

impl<W: io::Write> RawMcuEncoderStarted<W> {
    /// Write pre-downsampled component planes to the encoder.
    ///
    /// Provides raw YCbCr or grayscale component data that has already been color-converted
    /// and chroma-downsampled. The encoder processes these components directly without
    /// any color space conversion or downsampling.
    ///
    /// # Arguments
    ///
    /// * `components` - Slice of component buffers:
    ///   - **YCbCr mode**: `&[&y_data, &cb_data, &cr_data]` (3 components)
    ///   - **Grayscale mode**: `&[&y_data]` (1 component)
    ///
    /// # Component Sizes
    ///
    /// Each component buffer must contain exactly the number of bytes specified in the
    /// [`RawMcuConfig`]:
    ///
    /// For **YCbCr 4:2:0** (640×480 image):
    /// - Y component: `640 * 480 = 307,200` bytes
    /// - Cb component: `320 * 240 = 76,800` bytes (half width, half height)
    /// - Cr component: `320 * 240 = 76,800` bytes (half width, half height)
    ///
    /// For **YCbCr 4:2:2** (640×480 image):
    /// - Y component: `640 * 480 = 307,200` bytes
    /// - Cb component: `320 * 480 = 153,600` bytes (half width, full height)
    /// - Cr component: `320 * 480 = 153,600` bytes (half width, full height)
    ///
    /// For **YCbCr 4:4:4** (640×480 image):
    /// - Y component: `640 * 480 = 307,200` bytes
    /// - Cb component: `640 * 480 = 307,200` bytes (full resolution)
    /// - Cr component: `640 * 480 = 307,200` bytes (full resolution)
    ///
    /// For **Grayscale** (640×480 image):
    /// - Y component: `640 * 480 = 307,200` bytes
    ///
    /// # Errors
    ///
    /// Returns an I/O error if:
    /// - Wrong number of components (e.g., 2 components when 3 expected)
    /// - Component buffers are too small
    /// - Writing to the output fails
    ///
    /// # Example: YCbCr 4:2:0
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let width = 640;
    /// let height = 480;
    ///
    /// // Pre-downsampled component planes
    /// let y_plane = vec![128u8; width * height];          // Full resolution
    /// let cb_plane = vec![128u8; (width/2) * (height/2)]; // Half resolution
    /// let cr_plane = vec![128u8; (width/2) * (height/2)]; // Half resolution
    ///
    /// let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// config.input = JpegInput::RawMcu(RawMcuConfig::YCbCr {
    ///     subsampling: ChromaSubsampling::Yuv420,
    ///     y_size: (width, height),
    ///     cb_size: (width/2, height/2),
    ///     cr_size: (width/2, height/2),
    /// });
    ///
    /// let encoder = config.create_mcu_planar_encoder(width, height)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// // Write component planes in order: Y, Cb, Cr
    /// started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane])?;
    ///
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Example: Grayscale
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let width = 640;
    /// let height = 480;
    /// let y_plane = vec![128u8; width * height];
    ///
    /// let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// config.input = JpegInput::RawMcu(RawMcuConfig::Grayscale {
    ///     size: (width, height),
    /// });
    ///
    /// let encoder = config.create_mcu_planar_encoder(width, height)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// // Write single grayscale component
    /// started.write_raw_data(&[&y_plane])?;
    ///
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_raw_data(&mut self, components: &[&[u8]]) -> io::Result<()> {
        // Validate component count
        let expected_count = match &self.config.input {
            JpegInput::RawMcu(RawMcuConfig::YCbCr { .. }) => 3,
            JpegInput::RawMcu(RawMcuConfig::Grayscale { .. }) => 1,
            _ => unreachable!(),
        };

        if components.len() != expected_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Expected {} components, got {}", expected_count, components.len()),
            ));
        }

        self.compress.write_raw_data(components);
        Ok(())
    }

    /// Finish compression and return the output writer.
    ///
    /// Completes the JPEG encoding process by:
    /// 1. Flushing any remaining compressed data
    /// 2. Writing the JPEG end-of-image marker
    /// 3. Finalizing the output stream
    /// 4. Returning the underlying writer
    ///
    /// **Important**: You must call this method to produce a valid JPEG file.
    /// Dropping the encoder without calling `finish()` will result in an incomplete
    /// (and likely invalid) JPEG.
    ///
    /// # Returns
    ///
    /// The output writer that was provided to [`start()`](RawMcuEncoder::start).
    /// For `Vec<u8>` writers, this contains the complete JPEG data.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if finalizing the output fails (e.g., disk full, write error).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let y_plane = vec![128u8; 640 * 480];
    /// # let cb_plane = vec![128u8; 320 * 240];
    /// # let cr_plane = vec![128u8; 320 * 240];
    /// # let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// # config.input = JpegInput::RawMcu(RawMcuConfig::YCbCr {
    /// #     subsampling: ChromaSubsampling::Yuv420,
    /// #     y_size: (640, 480),
    /// #     cb_size: (320, 240),
    /// #     cr_size: (320, 240),
    /// # });
    /// let encoder = config.create_mcu_planar_encoder(640, 480)?;
    /// let mut started = encoder.start(Vec::new())?;
    /// started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane])?;
    ///
    /// // Finish encoding and get JPEG data
    /// let jpeg_bytes: Vec<u8> = started.finish()?;
    /// println!("Encoded {} bytes", jpeg_bytes.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn finish(self) -> io::Result<W> {
        self.compress.finish()
    }

    /// Get a reference to the encoder configuration.
    ///
    /// Returns the [`JpegConfig`] that was used to create this encoder. Useful for
    /// inspecting settings after encoding has started.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mozjpeg::typed::*;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// # config.input = JpegInput::RawMcu(RawMcuConfig::Grayscale { size: (640, 480) });
    /// let encoder = config.create_mcu_planar_encoder(640, 480)?;
    /// let started = encoder.start(Vec::new())?;
    ///
    /// // Access config during encoding
    /// println!("Quality: {}", started.config().quality);
    /// # Ok(())
    /// # }
    /// ```
    pub fn config(&self) -> &JpegConfig {
        &self.config
    }
}

// imgref support (feature-gated)
#[cfg(feature = "image_ref")]
use imgref::ImgRef;

#[cfg(feature = "image_ref")]
impl<W: io::Write> ScanlineEncoderStarted<W> {
    /// Write pixel data from an `ImgRef` or `ImgVec`.
    ///
    /// Convenience method for encoding images from the `imgref` crate. Automatically
    /// extracts dimensions and handles row stride from the image reference.
    ///
    /// This method works with:
    /// - `ImgRef<Pixel>` (borrowed image references)
    /// - `ImgVec<Pixel>` (owned images, via `.as_ref()`)
    /// - Any type that implements `AsRef<ImgRef<Pixel>>`
    ///
    /// # Pixel Type Requirements
    ///
    /// The `Pixel` type must implement `AsRef<[u8]>` to allow byte-level access.
    /// Common types that work:
    /// - `[u8; 3]` for RGB24
    /// - `[u8; 4]` for RGBA (alpha channel is ignored)
    /// - `u8` for grayscale
    ///
    /// # Stride Handling
    ///
    /// This method automatically handles non-contiguous image data (rows with padding).
    /// The stride is extracted from the `ImgRef` and passed to the encoder.
    ///
    /// # Arguments
    ///
    /// * `img` - Image reference from the `imgref` crate
    ///
    /// # Errors
    ///
    /// Returns an I/O error if:
    /// - Writing to the output fails
    /// - Image dimensions don't match encoder configuration
    /// - Internal compression error occurs
    ///
    /// # Example: RGB from ImgVec
    ///
    /// ```no_run
    /// # #[cfg(feature = "image_ref")] {
    /// use mozjpeg::typed::*;
    /// use imgref::ImgVec;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create RGB image (640×480)
    /// let pixels = vec![[255u8, 0, 0]; 640 * 480]; // Red pixels
    /// let img = ImgVec::new(pixels, 640, 480);
    ///
    /// // Encode directly from ImgVec
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(640, 480)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// started.write_imgref(&img.as_ref())?;
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    ///
    /// # Example: RGB from ImgRef
    ///
    /// ```no_run
    /// # #[cfg(feature = "image_ref")] {
    /// use mozjpeg::typed::*;
    /// use imgref::ImgRef;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // RGB pixels as [u8; 3] - implements AsRef<[u8]>
    /// let pixels = vec![[128u8, 64, 32]; 640 * 480];
    /// let img = ImgRef::new(&pixels, 640, 480);
    ///
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(640, 480)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// started.write_imgref(&img)?;
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    ///
    /// # Example: With stride
    ///
    /// ```no_run
    /// # #[cfg(feature = "image_ref")] {
    /// use mozjpeg::typed::*;
    /// use imgref::ImgRef;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create image with custom stride (e.g., from external buffer)
    /// let width = 640;
    /// let height = 480;
    /// let stride = 650; // Stride larger than width (padded rows)
    ///
    /// let mut buffer = vec![[0u8, 0, 0]; stride * height];
    /// // Fill buffer with RGB data...
    ///
    /// // Create ImgRef with custom stride
    /// let img = ImgRef::new_stride(&buffer, width, height, stride);
    ///
    /// let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
    /// let encoder = config.create_encoder(width, height)?;
    /// let mut started = encoder.start(Vec::new())?;
    ///
    /// // Stride is handled automatically
    /// started.write_imgref(&img)?;
    /// let jpeg = started.finish()?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`write_scanlines()`](Self::write_scanlines) for tightly-packed byte slices
    /// - [`write_scanlines_strided()`](Self::write_scanlines_strided) for manual stride handling
    pub fn write_imgref<Pixel: AsRef<[u8]>>(&mut self, img: &ImgRef<Pixel>) -> io::Result<()> {
        // Calculate stride in bytes
        let pixel_size = std::mem::size_of::<Pixel>();
        let stride = img.stride() * pixel_size;

        // Get buffer as bytes
        let buf = img.buf();
        #[allow(clippy::manual_slice_size_calculation)]
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                buf.as_ptr() as *const u8,
                buf.len() * pixel_size
            )
        };

        self.write_scanlines_strided(bytes, stride)
    }
}

