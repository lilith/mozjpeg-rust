//! Configuration validation and tracking for mozjpeg API
//!
//! This module provides tools to detect configuration ordering bugs at runtime.
//! The main problem: certain API calls (like `set_scan_optimization_mode()`) reset
//! previously-set configuration values, and this behavior is not obvious from the API.
//!
//! # Example
//!
//! ```
//! use mozjpeg::{Compress, ColorSpace, ScanMode};
//! use mozjpeg::validation::ConfigTracker;
//!
//! let mut comp = Compress::new(ColorSpace::JCS_RGB);
//! let mut tracker = ConfigTracker::new();
//!
//! comp.set_size(64, 64);
//! tracker.record_size(64, 64);
//!
//! comp.set_smoothing_factor(50);
//! tracker.record_smoothing(50);
//!
//! comp.set_scan_optimization_mode(ScanMode::Auto);
//! tracker.record_scan_mode(ScanMode::Auto);
//!
//! // Check for ordering problems
//! let warnings = tracker.check_order();
//! for warning in &warnings {
//!     eprintln!("Warning: {}", warning);
//! }
//! ```

use crate::{Compress, ScanMode};
use std::fmt;

/// Tracks configuration state to detect when settings get reset
#[derive(Debug, Clone)]
pub struct ConfigTracker {
    /// Expected smoothing factor
    smoothing_factor: Option<u8>,
    /// Expected raw_data_in flag
    raw_data_in: Option<bool>,
    /// Expected subsampling factors (Y h, Y v, Cb h, Cb v, Cr h, Cr v)
    subsampling: Option<[i32; 6]>,
    /// Expected quality
    quality: Option<f32>,
    /// Expected dimensions (width, height)
    dimensions: Option<(usize, usize)>,
    /// Whether progressive mode was set
    progressive: bool,
    /// Scan optimization mode
    scan_mode: Option<ScanMode>,
    /// Order of operations (for debugging)
    operation_log: Vec<ConfigOperation>,
}

/// Represents a configuration operation
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum ConfigOperation {
    SetSmoothing(u8),
    SetRawDataIn(bool),
    SetSubsampling([i32; 6]),
    SetQuality(f32),
    SetSize(usize, usize),
    SetProgressive,
    SetScanMode(ScanMode),
}

impl fmt::Display for ConfigOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigOperation::SetSmoothing(v) => write!(f, "set_smoothing_factor({})", v),
            ConfigOperation::SetRawDataIn(v) => write!(f, "set_raw_data_in({})", v),
            ConfigOperation::SetSubsampling(factors) => {
                write!(f, "set_subsampling({:?})", factors)
            }
            ConfigOperation::SetQuality(v) => write!(f, "set_quality({})", v),
            ConfigOperation::SetSize(w, h) => write!(f, "set_size({}, {})", w, h),
            ConfigOperation::SetProgressive => write!(f, "set_progressive_mode()"),
            ConfigOperation::SetScanMode(mode) => {
                write!(f, "set_scan_optimization_mode({:?})", mode)
            }
        }
    }
}

/// Validation error describing what went wrong
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    /// What setting was affected
    pub setting: String,
    /// Expected value
    pub expected: String,
    /// Actual value found
    pub actual: String,
    /// Likely cause
    pub cause: String,
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: expected '{}', got '{}' - {}",
            self.setting, self.expected, self.actual, self.cause
        )
    }
}

impl ConfigTracker {
    /// Create a new configuration tracker
    pub fn new() -> Self {
        Self {
            smoothing_factor: None,
            raw_data_in: None,
            subsampling: None,
            quality: None,
            dimensions: None,
            progressive: false,
            scan_mode: None,
            operation_log: Vec::new(),
        }
    }

    /// Record that smoothing factor was set
    pub fn record_smoothing(&mut self, value: u8) {
        self.smoothing_factor = Some(value);
        self.operation_log
            .push(ConfigOperation::SetSmoothing(value));
    }

    /// Record that raw_data_in was set
    pub fn record_raw_data_in(&mut self, value: bool) {
        self.raw_data_in = Some(value);
        self.operation_log
            .push(ConfigOperation::SetRawDataIn(value));
    }

    /// Record subsampling factors
    pub fn record_subsampling(&mut self, factors: [i32; 6]) {
        self.subsampling = Some(factors);
        self.operation_log
            .push(ConfigOperation::SetSubsampling(factors));
    }

    /// Record quality
    pub fn record_quality(&mut self, value: f32) {
        self.quality = Some(value);
        self.operation_log.push(ConfigOperation::SetQuality(value));
    }

    /// Record dimensions
    pub fn record_size(&mut self, width: usize, height: usize) {
        self.dimensions = Some((width, height));
        self.operation_log
            .push(ConfigOperation::SetSize(width, height));
    }

    /// Record progressive mode
    pub fn record_progressive(&mut self) {
        self.progressive = true;
        self.operation_log.push(ConfigOperation::SetProgressive);
    }

    /// Record scan optimization mode
    ///
    /// **IMPORTANT**: This call may reset other settings! The validator will check
    /// if any settings configured BEFORE this call are different AFTER this call.
    pub fn record_scan_mode(&mut self, mode: ScanMode) {
        self.scan_mode = Some(mode);
        self.operation_log.push(ConfigOperation::SetScanMode(mode));
    }

    /// Validate that current compress state matches tracked configuration
    ///
    /// Returns `Ok(())` if everything matches, or `Err` with a list of warnings
    /// if any settings have been reset or changed.
    #[cfg(test)]
    pub fn validate(&self, comp: &Compress) -> Result<(), Vec<ValidationWarning>> {
        let mut warnings = Vec::new();

        // Check smoothing factor
        if let Some(expected_smoothing) = self.smoothing_factor {
            let actual_smoothing = comp.cinfo().smoothing_factor as u8;
            if actual_smoothing != expected_smoothing {
                warnings.push(ValidationWarning {
                    setting: "smoothing_factor".to_string(),
                    expected: expected_smoothing.to_string(),
                    actual: actual_smoothing.to_string(),
                    cause: self.diagnose_cause(ConfigOperation::SetSmoothing(expected_smoothing)),
                });
            }
        }

        // Check raw_data_in
        if let Some(expected_raw) = self.raw_data_in {
            let actual_raw = comp.cinfo().raw_data_in != 0;
            if actual_raw != expected_raw {
                warnings.push(ValidationWarning {
                    setting: "raw_data_in".to_string(),
                    expected: expected_raw.to_string(),
                    actual: actual_raw.to_string(),
                    cause: self.diagnose_cause(ConfigOperation::SetRawDataIn(expected_raw)),
                });
            }
        }

        // Check subsampling
        if let Some(expected_subsampling) = self.subsampling {
            let comps = comp.components();
            if comps.len() >= 3 {
                let actual = [
                    comps[0].h_samp_factor,
                    comps[0].v_samp_factor,
                    comps[1].h_samp_factor,
                    comps[1].v_samp_factor,
                    comps[2].h_samp_factor,
                    comps[2].v_samp_factor,
                ];
                if actual != expected_subsampling {
                    warnings.push(ValidationWarning {
                        setting: "subsampling_factors".to_string(),
                        expected: format!("{:?}", expected_subsampling),
                        actual: format!("{:?}", actual),
                        cause: self
                            .diagnose_cause(ConfigOperation::SetSubsampling(expected_subsampling)),
                    });
                }
            }
        }

        if warnings.is_empty() {
            Ok(())
        } else {
            Err(warnings)
        }
    }

    /// Diagnose the likely cause of a setting being reset
    #[allow(dead_code)]
    fn diagnose_cause(&self, setting: ConfigOperation) -> String {
        // Find when this setting was last set
        let setting_index = self.operation_log.iter().rposition(|op| op == &setting);

        if let Some(idx) = setting_index {
            // Check if set_scan_optimization_mode was called after this setting
            let scan_mode_after = self.operation_log[idx..]
                .iter()
                .any(|op| matches!(op, ConfigOperation::SetScanMode(_)));

            if scan_mode_after {
                return "likely reset by set_scan_optimization_mode() which calls jpeg_set_defaults()".to_string();
            }
        }

        "unknown cause".to_string()
    }

    /// Get the operation log for debugging
    pub fn operation_log(&self) -> &[ConfigOperation] {
        &self.operation_log
    }

    /// Print a human-readable summary of the configuration order
    pub fn print_summary(&self) {
        println!("Configuration order:");
        for (i, op) in self.operation_log.iter().enumerate() {
            println!("  {}. {}", i + 1, op);
        }
    }

    /// Check if the current configuration order is likely to cause bugs
    ///
    /// Returns warnings about potentially problematic orderings
    pub fn check_order(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Find index of set_scan_optimization_mode
        let scan_mode_idx = self
            .operation_log
            .iter()
            .position(|op| matches!(op, ConfigOperation::SetScanMode(_)));

        if let Some(scan_idx) = scan_mode_idx {
            // Check if any "resetable" settings were configured before scan_mode
            for (i, op) in self.operation_log.iter().enumerate() {
                if i >= scan_idx {
                    break; // Only check operations BEFORE scan_mode
                }

                match op {
                    ConfigOperation::SetSmoothing(_) => {
                        // Check if it was set again AFTER scan_mode
                        let set_after = self.operation_log[scan_idx..]
                            .iter()
                            .any(|o| matches!(o, ConfigOperation::SetSmoothing(_)));

                        if !set_after {
                            warnings.push(format!(
                                "smoothing_factor set before set_scan_optimization_mode() at step {}. \
                                 It may be reset (but is preserved on this branch with the fix).",
                                i + 1
                            ));
                        }
                    }
                    ConfigOperation::SetRawDataIn(_) => {
                        let set_after = self.operation_log[scan_idx..]
                            .iter()
                            .any(|o| matches!(o, ConfigOperation::SetRawDataIn(_)));

                        if !set_after {
                            warnings.push(format!(
                                "raw_data_in set before set_scan_optimization_mode() at step {}. \
                                 It WILL be reset!",
                                i + 1
                            ));
                        }
                    }
                    ConfigOperation::SetSubsampling(_) => {
                        let set_after = self.operation_log[scan_idx..]
                            .iter()
                            .any(|o| matches!(o, ConfigOperation::SetSubsampling(_)));

                        if !set_after {
                            warnings.push(format!(
                                "subsampling_factors set before set_scan_optimization_mode() at step {}. \
                                 They WILL be reset!",
                                i + 1
                            ));
                        }
                    }
                    ConfigOperation::SetProgressive => {
                        let set_after = self.operation_log[scan_idx..]
                            .iter()
                            .any(|o| matches!(o, ConfigOperation::SetProgressive));

                        if !set_after {
                            warnings.push(format!(
                                "progressive mode set before set_scan_optimization_mode() at step {}. \
                                 It may be affected!",
                                i + 1
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        warnings
    }
}

impl Default for ConfigTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColorSpace;

    #[test]
    fn detect_subsampling_reset() {
        let mut comp = Compress::new(ColorSpace::JCS_RGB);
        comp.set_size(64, 64);
        comp.set_color_space(ColorSpace::JCS_YCbCr);

        let mut tracker = ConfigTracker::new();
        tracker.record_size(64, 64);

        // Set subsampling BEFORE scan mode (wrong order - will be reset)
        {
            let comps = comp.components_mut();
            comps[0].h_samp_factor = 2;
            comps[0].v_samp_factor = 1;
            comps[1].h_samp_factor = 1;
            comps[1].v_samp_factor = 1;
            comps[2].h_samp_factor = 1;
            comps[2].v_samp_factor = 1;
        }
        tracker.record_subsampling([2, 1, 1, 1, 1, 1]);

        comp.set_scan_optimization_mode(ScanMode::Auto);
        tracker.record_scan_mode(ScanMode::Auto);

        // Should detect that subsampling was reset
        let result = tracker.validate(&comp);
        assert!(result.is_err(), "Should detect subsampling reset");

        let warnings = result.unwrap_err();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].setting, "subsampling_factors");
        assert!(warnings[0].cause.contains("jpeg_set_defaults"));
    }

    #[test]
    fn order_check_warns_about_problems() {
        let mut tracker = ConfigTracker::new();

        tracker.record_smoothing(50);
        tracker.record_subsampling([2, 1, 1, 1, 1, 1]);
        tracker.record_scan_mode(ScanMode::Auto);

        let warnings = tracker.check_order();

        // Should warn about subsampling being set before scan_mode
        assert!(!warnings.is_empty(), "Should warn about ordering issues");

        let subsampling_warning = warnings.iter().find(|w| w.contains("subsampling"));
        assert!(
            subsampling_warning.is_some(),
            "Should warn about subsampling"
        );
    }
}
