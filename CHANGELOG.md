# Changelog

## [0.11.0] - 2025-02-04

### Breaking Changes

- **Full lazy configuration**: ALL settings are now collected and applied at `start_compress()` time instead of immediately. This fixes all configuration ordering bugs but changes the semantics:
  - All setter methods (including `set_size()`, `set_color_space()`) store to a pending configuration
  - All settings are applied in the correct order when `start_compress()` is called
  - Getters like `components()` return current `cinfo` values, which won't reflect pending config until `start_compress()` (or until deprecated `components_mut()` is called)

- **Deprecated `components_mut()`**: Use `mutate_components_last()` instead for order-independent configuration. The old method still works but will apply pending config early when called.

### Fixed

- Configuration ordering bugs where `set_scan_optimization_mode()` and `set_fastest_defaults()` would reset other settings via internal `jpeg_set_defaults()` calls. Settings like quality, smoothing, subsampling, pixel density, and progressive mode are now preserved regardless of call order.

### Added

- `mutate_components_last()` - Modify components via a callback that runs after all other configuration is applied
- `mutate_cinfo_last()` - Access raw `cinfo` via a callback that runs after all other configuration is applied
- `PendingConfig` struct (internal, `pub(crate)`) to collect deferred settings with incremental application via snapshots
- `Clone` derive for `QTable`
- `Copy`, `Clone`, `Debug`, `PartialEq`, `Eq` derives for `PixelDensity` and `PixelDensityUnit`

### Migration Guide

Before 0.11.0, the order of configuration calls mattered:

```rust
// BROKEN in 0.10.x: smoothing gets reset to 0!
comp.set_smoothing_factor(50);
comp.set_scan_optimization_mode(ScanMode::Auto);
```

In 0.11.0, order no longer matters:

```rust
// Both orderings produce identical results in 0.11.0
comp.set_smoothing_factor(50);
comp.set_scan_optimization_mode(ScanMode::Auto);

// Same result as:
comp.set_scan_optimization_mode(ScanMode::Auto);
comp.set_smoothing_factor(50);
```

**Migrating from `components_mut()`**: Use `mutate_components_last()` instead:

```rust
// Before (0.10.x):
comp.components_mut()[0].h_samp_factor = 2;

// After (0.11.0):
comp.mutate_components_last(|components| {
    components[0].h_samp_factor = 2;
});
```

**Note**: The deprecated `components_mut()` method will apply pending config when called, so any setter calls made AFTER `components_mut()` will have no effect. Use `mutate_components_last()` for full order-independence.

## [0.10.13] and earlier

See git history for previous changes.
