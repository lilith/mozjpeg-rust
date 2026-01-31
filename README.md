# Rust wrapper for MozJPEG library

This library adds a safe(r) interface on top of libjpeg-turbo and MozJPEG for reading and writing well-compressed JPEG images.

The interface is still being developed, so it has rough edges and may change.

In particular, error handling is weird due to libjpeg's peculiar design. Error handling can't use `Result`, but needs to depend on Rust's `resume_unwind` (a panic, basically) to signal any errors in libjpeg. It's necessary to wrap all uses of this library in `catch_unwind`.

In crates compiled with `panic=abort` setting, any JPEG error will abort the process.

## Decoding example

```rust
std::panic::catch_unwind(|| -> std::io::Result<Vec<rgb::Rgb::<u8>>> {
    let d = mozjpeg::Decompress::with_markers(mozjpeg::ALL_MARKERS)
        .from_path("tests/test.jpg")?;

    d.width(); // FYI
    d.height();
    d.color_space() == mozjpeg::ColorSpace::JCS_YCbCr;
    for marker in d.markers() { /* read metadata or color profiles */ }

    // rgb() enables conversion
    let mut image = d.rgb()?;
    image.width();
    image.height();
    image.color_space() == mozjpeg::ColorSpace::JCS_RGB;

    let pixels = image.read_scanlines()?;
    image.finish()?;
    Ok(pixels)
});
```

## Encoding example

```rust
# let width = 8; let height = 8;
std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);

    comp.set_size(width, height);
    let mut comp = comp.start_compress(Vec::new())?; // any io::Write will work

    // replace with your image data
    let pixels = vec![0u8; width * height * 3];
    comp.write_scanlines(&pixels[..])?;

    let writer = comp.finish()?;
    Ok(writer)
});
```

## Setting Compatibility

Not all settings can be combined. The underlying libjpeg/mozjpeg library has implicit constraints that are checked at `start_compress()` or later.

### Compression Settings

| Setting | Incompatible With | Behavior |
|---------|-------------------|----------|
| `raw_data_in = true` | `write_scanlines()` | Must use `write_raw_data()` instead |
| `raw_data_in = true` | Color conversion | Bypasses all preprocessing |
| `smoothing_factor > 0` | Non-standard sampling (e.g., 4:2:2, 4:4:4) | Silently ignored (warning only) |
| Progressive/multi-scan | I/O suspension | Cannot suspend during `finish_compress()` |
| Huffman optimization | I/O suspension | Cannot suspend during multi-pass encoding |

### Sampling Factor Rules

- At least one component must have `h_samp_factor = 1`
- At least one component must have `v_samp_factor = 1`
- Fractional sampling ratios are not supported

### Decompression Settings

| Setting | Incompatible With | Behavior |
|---------|-------------------|----------|
| `raw_data_out = true` | Rescaling | Must accept native resolution |
| `raw_data_out = true` | Color quantization | Error |
| `raw_data_out = true` | `read_scanlines()` | Must use `read_raw_data()` instead |

### MozJPEG Extension Settings

| Setting | Notes |
|---------|-------|
| `optimize_scans` | Forces progressive mode |
| `trellis_quant` | Requires multi-pass (implies `optimize_coding`) |
| `use_scans_in_trellis` | Only effective when `trellis_quant = true` |

### Configuration Call Order

**Important:** Some settings interact with `jpeg_set_defaults()` which can reset previously-configured values. For reliable results:

```rust,ignore
// Recommended order
let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
comp.set_size(width, height);
comp.set_scan_optimization_mode(ScanMode::Auto);  // Call FIRST if using
comp.set_quality(85.0);                            // Then other settings
comp.set_smoothing_factor(50);
// ... remaining configuration
let mut comp = comp.start_compress(writer)?;
```

See [ORDERING_BUGS.md](ORDERING_BUGS.md) for details on configuration ordering issues.
