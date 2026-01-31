//! Type-safe JPEG encoding API
//!
//! This module provides a type-safe alternative to the main mozjpeg API where
//! invalid configurations are unrepresentable in the type system.
//!
//! # Key differences from main API
//!
//! - **Input mode is explicit**: `JpegInput::Scanlines` vs `JpegInput::RawMcu`
//! - **Color space combinations are validated**: Only valid combos exist as enum variants
//! - **Settings have constraints**: `smoothing_factor` only in scanline mode, `optimize_scans` only in progressive
//! - **Separate encoder types**: `ScanlineEncoder` and `RawMcuEncoder` with appropriate methods
//!
//! # Examples
//!
//! ## Easy mode: Use a preset
//!
//! ```no_run
//! use mozjpeg::typed::*;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let rgb_pixels = vec![255u8; 640 * 480 * 3];
//!
//! // Recommended: Progressive with optimizations at quality 85
//! let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
//!
//! // No matching needed - create_encoder() returns concrete ScanlineEncoder
//! let encoder = config.create_encoder(640, 480)?;
//! let mut started = encoder.start(Vec::new())?;
//! started.write_scanlines(&rgb_pixels)?;
//! let jpeg = started.finish()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Other presets
//!
//! ```no_run
//! use mozjpeg::typed::*;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let rgb_pixels = vec![255u8; 640 * 480 * 3];
//! // Fastest encoding (no optimizations)
//! let config = JpegConfig::from_preset(Preset::SequentialFastest, 85.0);
//!
//! // Sequential with optimizations (for sequential decode)
//! let config = JpegConfig::from_preset(Preset::SequentialBalanced, 85.0);
//!
//! // Maximum compression (adds ~100% time for ~1% size reduction)
//! let config = JpegConfig::from_preset(Preset::ProgressiveSmallest, 75.0);
//! # Ok(())
//! # }
//! ```
//!
//! ## Simple: One-liner RGB encoding
//!
//! ```no_run
//! use mozjpeg::typed::*;
//!
//! # fn example() -> std::io::Result<()> {
//! let rgb_pixels = vec![255u8; 640 * 480 * 3];
//! let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
//! let jpeg = config.encode_rgb(&rgb_pixels, 640, 480)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Strided pixel data
//!
//! ```no_run
//! use mozjpeg::typed::*;
//!
//! # fn example() -> std::io::Result<()> {
//! // Pixel data with padding/alignment between rows
//! let width = 640;
//! let height = 480;
//! let stride = 1024; // bytes per row (may be > width * 3 for alignment)
//! let pixels = vec![0u8; stride * height];
//!
//! let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
//! let jpeg = config.encode_rgb_strided(&pixels, width, height, stride)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Using imgref (feature-gated)
//!
//! ```no_run
//! # #[cfg(feature = "image_ref")]
//! # fn example() -> std::io::Result<()> {
//! use mozjpeg::typed::*;
//! use imgref::ImgVec;
//!
//! let width = 640;
//! let height = 480;
//! let pixels = vec![[255u8, 0, 0]; width * height]; // RGB pixels
//! let img = ImgVec::new(pixels, width, height);
//!
//! // Dimensions extracted automatically from imgref
//! let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
//! let jpeg = config.encode_imgref(&img.as_ref())?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Advanced configuration
//!
//! ```
//! use mozjpeg::typed::*;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = JpegConfig {
//!     input: JpegInput::Scanlines(
//!         ScanlineConfig::RgbToYCbCr {
//!             subsampling: ChromaSubsampling::Yuv420,
//!             smoothing: 50,
//!         }
//!     ),
//!     compression: CompressionMode::Progressive {
//!         optimize_scans: true,
//!         use_scans_in_trellis: false,
//!     },
//!     qtables: QTableConfig::FromQuality,
//!     quality: 85.0,
//!     optimize_coding: true,
//!     scan_mode: mozjpeg::ScanMode::Auto,
//!     force_8bit_quantization: false,
//! };
//!
//! // No matching - create_encoder() returns concrete type
//! let encoder = config.create_encoder(640, 480)?;
//! let rgb_pixels = vec![255u8; 640 * 480 * 3];
//! let mut started = encoder.start(Vec::new())?;
//! started.write_scanlines(&rgb_pixels)?;
//! let jpeg = started.finish()?;
//! # Ok(())
//! # }
//! ```

mod config;
mod encoder;

#[cfg(test)]
mod tests;

// Re-export main types
pub use config::*;
pub use encoder::*;
