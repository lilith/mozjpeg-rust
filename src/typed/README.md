# Type-Safe JPEG Encoding API

A type-safe, builder-style API for MozJPEG encoding where invalid configurations are unrepresentable at compile time.

## Why Use the Typed API?

### Main API (Raw FFI wrapper)
```rust
let mut compress = Compress::new(ColorSpace::JCS_RGB);
compress.set_size(width, height);
compress.set_color_space(ColorSpace::JCS_YCbCr); // Wait, which color space?
compress.set_quality(85.0);
// Did I forget to set something? Will this work?
```

### Typed API (Type-safe builder)
```rust
let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
let jpeg = config.encode_rgb(&pixels, 640, 480)?;
// All required settings are configured, invalid states are impossible
```

## Key Features

- **Valid-by-construction**: Color space combinations, subsampling modes, and input formats are all enum variants — you can't express an invalid combination
- **No matching needed**: Encoder methods return concrete types
- **Builder pattern**: Fluent API with `with_*()` methods
- **Presets**: Optimized defaults for common use cases
- **Stride support**: Handle padded/aligned pixel data
- **imgref integration**: First-class support for `imgref` crate (feature-gated)
- **Comprehensive docs**: Every type and method fully documented

## Quick Start

### One-Line Encoding

```rust
use mozjpeg::typed::*;

let pixels = vec![255u8; 640 * 480 * 3]; // RGB pixels
let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
let jpeg = config.encode_rgb(&pixels, 640, 480)?;
```

### With Configuration

```rust
use mozjpeg::typed::*;

let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0)
    .with_quality(90.0)
    .with_smoothing(25);

let encoder = config.create_encoder(640, 480)?;
let mut started = encoder.start(Vec::new())?;
started.write_scanlines(&pixels)?;
let jpeg = started.finish()?;
```

### Advanced: Manual Configuration

```rust
use mozjpeg::typed::*;

let config = JpegConfig {
    input: JpegInput::Scanlines(
        ScanlineConfig::RgbToYCbCr {
            subsampling: ChromaSubsampling::Yuv420,
            smoothing: 50,
        }
    ),
    compression: CompressionMode::Progressive {
        optimize_scans: true,
        use_scans_in_trellis: false,
    },
    qtables: QTableConfig::FromQuality,
    quality: 85.0,
    optimize_coding: true,
    scan_mode: mozjpeg::ScanMode::Auto,
    force_8bit_quantization: false,
};

let encoder = config.create_encoder(640, 480)?;
```

## Presets Explained

Choose the preset that matches your requirements:

| Preset | Progressive | Huffman Opt | Optimize Scans | Use Case |
|--------|------------|-------------|----------------|----------|
| `SequentialFastest` | No | No | No | Real-time, thumbnails |
| `SequentialBalanced` | No | Yes | No | Sequential decode needed |
| `ProgressiveBalanced` | Yes | Yes | No | **Recommended default** |
| `ProgressiveSmallest` | Yes | Yes | Yes | File size critical |

**Recommendation**: Use `ProgressiveBalanced` for most cases. Only use `ProgressiveSmallest` when you need maximum compression and can afford 2x slower encoding.

## Chroma Subsampling Guide

JPEG can downsample color information (chroma) while keeping brightness (luma) at full resolution.

| Mode | Notation | Description | File Size | Quality | Use Case |
|------|----------|-------------|-----------|---------|----------|
| `Yuv444` | 4:4:4 | No subsampling | Largest | Best | Screenshots, text, graphics |
| `Yuv422` | 4:2:2 | Horizontal 2x | Medium | Good | Broadcast video |
| `Yuv420` | 4:2:0 | Both 2x | Smallest | Acceptable | **Photos (recommended)** |
| `Yuv411` | 4:1:1 | Horizontal 4x | Very small | Poor | Legacy video |

**For a 640×480 image:**
- **4:4:4**: Y=307,200 + Cb=307,200 + Cr=307,200 = 921,600 bytes
- **4:2:2**: Y=307,200 + Cb=153,600 + Cr=153,600 = 614,400 bytes
- **4:2:0**: Y=307,200 + Cb=76,800 + Cr=76,800 = 460,800 bytes

**Recommendation**: Use `Yuv420` for photos, `Yuv444` for screenshots/text.

## Strided Pixel Data

If your pixel data has padding between rows (common with memory-aligned buffers):

```rust
let width = 640;
let height = 480;
let stride = 1024; // bytes per row (includes padding)
let pixels = vec![0u8; stride * height];

let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
let jpeg = config.encode_rgb_strided(&pixels, width, height, stride)?;
```

Or with manual control:

```rust
let encoder = config.create_encoder(width, height)?;
let mut started = encoder.start(Vec::new())?;
started.write_scanlines_strided(&pixels, stride)?;
let jpeg = started.finish()?;
```

## imgref Support (Feature-Gated)

Enable the `image_ref` feature:

```toml
[dependencies]
mozjpeg = { version = "0.10", features = ["image_ref"] }
```

Then use imgref directly:

```rust
use mozjpeg::typed::*;
use imgref::ImgVec;

let width = 640;
let height = 480;
let pixels = vec![[255u8, 0, 0]; width * height]; // RGB pixels
let img = ImgVec::new(pixels, width, height);

// Dimensions and stride extracted automatically
let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
let jpeg = config.encode_imgref(&img.as_ref())?;
```

With custom stride:

```rust
let stride = 1024;
let img = ImgVec::new_stride(pixels, width, height, stride);
let jpeg = config.encode_imgref(&img.as_ref())?; // Stride handled automatically
```

## Advanced: Raw MCU Mode

For video codecs or custom processing pipelines that provide pre-downsampled YCbCr components:

```rust
use mozjpeg::typed::*;

let mut config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
config.input = JpegInput::RawMcu(
    RawMcuConfig::YCbCr {
        subsampling: ChromaSubsampling::Yuv420,
        y_size: (640, 480),
        cb_size: (320, 240),
        cr_size: (320, 240),
    }
);

let encoder = config.create_mcu_planar_encoder(640, 480)?;
let mut started = encoder.start(Vec::new())?;

// Provide pre-downsampled components
let y_plane = vec![128u8; 640 * 480];
let cb_plane = vec![128u8; 320 * 240]; // 2x downsampled
let cr_plane = vec![128u8; 320 * 240]; // 2x downsampled

started.write_raw_data(&[&y_plane, &cb_plane, &cr_plane])?;
let jpeg = started.finish()?;
```

## API Overview

### Configuration Types

- **`JpegConfig`**: Main configuration struct
  - `from_preset()` - Create from preset with quality
  - `rgb_to_ycbcr_420()` - RGB → YCbCr 4:2:0 (most common)
  - `rgb_to_ycbcr_444()` - RGB → YCbCr 4:4:4 (high quality)
  - `grayscale()` - Grayscale encoding
  - Builder methods: `with_quality()`, `with_progressive()`, `with_smoothing()`

- **`Preset`**: Encoding presets
  - `SequentialFastest` - No optimizations (4-10x faster)
  - `SequentialBalanced` - Sequential with Huffman opt
  - `ProgressiveBalanced` - Progressive with Huffman opt (recommended)
  - `ProgressiveSmallest` - Maximum compression (2x slower)

- **`ChromaSubsampling`**: Chroma downsampling modes
  - `Yuv444` - No subsampling (best quality)
  - `Yuv422` - Horizontal 2x
  - `Yuv420` - Both 2x (recommended for photos)
  - `Yuv411` - Horizontal 4x (legacy)

- **`ScanlineConfig`**: Input → JPEG color space mappings
  - `RgbToYCbCr` - RGB input with chroma subsampling
  - `RgbToRgb` - RGB passthrough (no conversion)
  - `YCbCrToYCbCr` - YCbCr input with subsampling
  - `Grayscale` - Single channel
  - `CmykToCmyk` - CMYK for print

- **`RawMcuConfig`**: Advanced pre-downsampled components
  - `YCbCr` - Three-component with subsampling
  - `Grayscale` - Single component

### Encoder Types

- **`ScanlineEncoder`**: Standard encoder (most common)
  - Created via `config.create_encoder(width, height)`
  - Returns concrete type, no matching needed
  - Handles color conversion and downsampling

- **`RawMcuEncoder`**: Advanced encoder (pre-downsampled)
  - Created via `config.create_mcu_planar_encoder(width, height)`
  - Requires pre-downsampled YCbCr components
  - ~10-20% faster when you already have YCbCr

### Encoding Methods

**On `JpegConfig`:**
- `encode_rgb(&pixels, width, height)` - One-call RGB encoding
- `encode_rgb_strided(&pixels, width, height, stride)` - With stride
- `encode_imgref(&img)` - From imgref (feature-gated)

**On `ScanlineEncoderStarted`:**
- `write_scanlines(&data)` - Tightly packed pixels
- `write_scanlines_strided(&data, stride)` - Custom row stride
- `write_imgref(&img)` - From imgref (feature-gated)
- `finish()` - Finalize and return output

**On `RawMcuEncoderStarted`:**
- `write_raw_data(&[&y, &cb, &cr])` - Pre-downsampled components
- `finish()` - Finalize and return output

## Common Patterns

### Pattern 1: Quality-optimized web images

```rust
let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0);
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

### Pattern 2: Maximum compression for archival

```rust
let config = JpegConfig::from_preset(Preset::ProgressiveSmallest, 75.0);
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

### Pattern 3: Real-time thumbnail generation

```rust
let config = JpegConfig::from_preset(Preset::SequentialFastest, 80.0);
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

### Pattern 4: High-quality screenshots

```rust
let config = JpegConfig::rgb_to_ycbcr_444(95.0)
    .with_progressive();
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

### Pattern 5: Grayscale photos

```rust
let config = JpegConfig::grayscale(85.0)
    .with_progressive();
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

### Pattern 6: Custom smoothing for gradients

```rust
let config = JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0)
    .with_smoothing(50); // 0-100, reduces color banding
let jpeg = config.encode_rgb(&pixels, width, height)?;
```

## Error Handling

The typed API validates configuration at encoder creation time. Mismatched input
modes, invalid dimensions, and out-of-range quality are all caught before encoding
starts:

```rust
let config = JpegConfig::rgb_to_ycbcr_420(85.0);

// This returns Err(ConfigError::WrongInputMode) — scanline config
// can't be used with the raw MCU encoder
let encoder = config.create_mcu_planar_encoder(640, 480);
assert!(encoder.is_err());

// Use the matching encoder instead
let encoder = config.create_encoder(640, 480)?; // Returns ScanlineEncoder
```

`ConfigError` variants:
- **`WrongInputMode`**: Scanline config passed to `create_mcu_planar_encoder()` or vice versa
- **`InvalidDimensions`**: Width or height is 0
- **`InvalidQuality`**: Quality not in 0.0–100.0
- **`InvalidMcuDimensions`**: Raw MCU dimensions not aligned to MCU boundaries
- **`InvalidComponentSize`**: Component plane dimensions don't match subsampling ratio

## Performance Notes

**Relative encoding cost by preset** (lower presets are faster):

1. `SequentialFastest` — no optimizations, fastest encoding
2. `SequentialBalanced` — adds Huffman optimization (two-pass)
3. `ProgressiveBalanced` — progressive + Huffman (similar cost to sequential balanced)
4. `ProgressiveSmallest` — adds scan optimization (roughly 2x slower than balanced)

Actual times depend on image content, dimensions, and hardware. Profile with your
workload rather than relying on generic numbers.

**Raw MCU mode** bypasses color conversion and downsampling. This can save
meaningful time when you already have YCbCr planes (e.g., from a video codec),
but you must provide correctly downsampled components.

## Feature Flags

### `image_ref` (optional)

Enables imgref integration:

```toml
[dependencies]
mozjpeg = { version = "0.10", features = ["image_ref"] }
```

Adds methods:
- `JpegConfig::encode_imgref()`
- `JpegConfig::create_encoder_from_imgref()`
- `ScanlineEncoderStarted::write_imgref()`

## API Documentation

Full API documentation is available via rustdoc:

```bash
cargo doc --open --package mozjpeg
```

Navigate to the `typed` module for complete documentation of all types and methods.

## Examples

See the [tests.rs](./tests.rs) file for comprehensive examples of:
- All presets
- All chroma subsampling modes
- Stride handling
- Progressive encoding
- Custom quantization tables
- Raw MCU mode
- imgref integration

## License

This module is part of the mozjpeg-rust crate and shares the same IJG license.

## Contributing

This is a new API added in mozjpeg-rust 0.11. Feedback and contributions welcome!

- **Report issues**: https://github.com/ImageOptim/mozjpeg-rust/issues
- **Suggestions**: Open an issue or PR

---

**Quick Reference Card**

| Task | Code |
|------|------|
| Basic RGB encoding | `JpegConfig::from_preset(Preset::ProgressiveBalanced, 85.0).encode_rgb(&pixels, w, h)?` |
| With stride | `config.encode_rgb_strided(&pixels, w, h, stride)?` |
| From imgref | `config.encode_imgref(&img.as_ref())?` |
| High quality | `JpegConfig::rgb_to_ycbcr_444(95.0).encode_rgb(&pixels, w, h)?` |
| Maximum compression | `JpegConfig::from_preset(Preset::ProgressiveSmallest, 75.0).encode_rgb(&pixels, w, h)?` |
| Fastest encoding | `JpegConfig::from_preset(Preset::SequentialFastest, 85.0).encode_rgb(&pixels, w, h)?` |
| Grayscale | `JpegConfig::grayscale(85.0).encode_rgb(&pixels, w, h)?` |
| With smoothing | `config.with_smoothing(50).encode_rgb(&pixels, w, h)?` |
