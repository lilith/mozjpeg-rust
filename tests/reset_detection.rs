//! Comprehensive struct-level reset detection for jpeg_set_defaults()
//!
//! Unlike the combinatorial_ordering tests (which compare output bytes),
//! these tests inspect actual cinfo struct fields to detect ALL resets,
//! including ones that don't affect output for a given test pattern
//! (e.g., optimize_scans without progressive mode enabled).
//!
//! Two functions in this crate call jpeg_set_defaults() internally:
//! - set_scan_optimization_mode()
//! - set_fastest_defaults()
//!
//! jpeg_set_defaults() resets (from jcparam.c):
//!   - smoothing_factor = 0
//!   - raw_data_in = FALSE
//!   - optimize_coding = FALSE  (but mozjpeg profile may re-enable)
//!   - arith_code = FALSE
//!   - CCIR601_sampling = FALSE
//!   - do_fancy_downsampling = TRUE
//!   - dct_method = JDCT_DEFAULT
//!   - restart_interval = 0
//!   - restart_in_rows = 0
//!   - density_unit = 0, X_density = 1, Y_density = 1
//!   - scan_info = NULL, num_scans = 0
//!   - Calls jpeg_set_quality(75, TRUE) → resets quantization tables
//!   - Calls jpeg_default_colorspace() → resets component sampling factors,
//!     quant_tbl_no, dc_tbl_no, ac_tbl_no via SET_COMP macro
//!   - Resets mozjpeg master fields: optimize_scans, use_scans_in_trellis,
//!     trellis_quant, dc_scan_opt_mode, etc.

use mozjpeg::*;

/// Snapshot of all user-facing cinfo fields that jpeg_set_defaults() might reset
#[derive(Debug, Clone)]
struct CinfoSnapshot {
    // Direct cinfo fields
    smoothing_factor: i32,
    raw_data_in: i32,
    optimize_coding: i32,
    density_unit: u8,
    x_density: u16,
    y_density: u16,
    num_scans: i32,
    // Component fields (for 3-component YCbCr)
    comp_h_samp: [i32; 3],
    comp_v_samp: [i32; 3],
    comp_quant_tbl_no: [i32; 3],
    comp_dc_tbl_no: [i32; 3],
    comp_ac_tbl_no: [i32; 3],
    // MozJPEG extension fields (queried via get_*_param)
    optimize_scans: bool,
    use_scans_in_trellis: bool,
    dc_scan_opt_mode: i32,
    compress_profile: i32,
}

impl CinfoSnapshot {
    fn capture(comp: &Compress) -> Self {
        let cinfo = comp.cinfo();
        let comps = comp.components();

        let mut comp_h_samp = [0i32; 3];
        let mut comp_v_samp = [0i32; 3];
        let mut comp_quant_tbl_no = [0i32; 3];
        let mut comp_dc_tbl_no = [0i32; 3];
        let mut comp_ac_tbl_no = [0i32; 3];

        for (i, c) in comps.iter().enumerate().take(3) {
            comp_h_samp[i] = c.h_samp_factor;
            comp_v_samp[i] = c.v_samp_factor;
            comp_quant_tbl_no[i] = c.quant_tbl_no;
            comp_dc_tbl_no[i] = c.dc_tbl_no;
            comp_ac_tbl_no[i] = c.ac_tbl_no;
        }

        let optimize_scans = unsafe {
            mozjpeg_sys::jpeg_c_get_bool_param(
                cinfo,
                mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS,
            ) != 0
        };
        let use_scans_in_trellis = unsafe {
            mozjpeg_sys::jpeg_c_get_bool_param(
                cinfo,
                mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_USE_SCANS_IN_TRELLIS,
            ) != 0
        };
        let dc_scan_opt_mode = unsafe {
            mozjpeg_sys::jpeg_c_get_int_param(
                cinfo,
                mozjpeg_sys::J_INT_PARAM::JINT_DC_SCAN_OPT_MODE,
            )
        };
        let compress_profile = unsafe {
            mozjpeg_sys::jpeg_c_get_int_param(
                cinfo,
                mozjpeg_sys::J_INT_PARAM::JINT_COMPRESS_PROFILE,
            )
        };

        Self {
            smoothing_factor: cinfo.smoothing_factor,
            raw_data_in: cinfo.raw_data_in,
            optimize_coding: cinfo.optimize_coding,
            density_unit: cinfo.density_unit,
            x_density: cinfo.X_density,
            y_density: cinfo.Y_density,
            num_scans: cinfo.num_scans,
            comp_h_samp,
            comp_v_samp,
            comp_quant_tbl_no,
            comp_dc_tbl_no,
            comp_ac_tbl_no,
            optimize_scans,
            use_scans_in_trellis,
            dc_scan_opt_mode,
            compress_profile,
        }
    }

    /// Compare two snapshots, return list of (field_name, before, after) for changed fields
    fn diff(&self, after: &Self) -> Vec<(String, String, String)> {
        let mut diffs = Vec::new();
        macro_rules! check {
            ($field:ident) => {
                if self.$field != after.$field {
                    diffs.push((
                        stringify!($field).to_string(),
                        format!("{:?}", self.$field),
                        format!("{:?}", after.$field),
                    ));
                }
            };
        }
        check!(smoothing_factor);
        check!(raw_data_in);
        check!(optimize_coding);
        check!(density_unit);
        check!(x_density);
        check!(y_density);
        check!(num_scans);
        check!(comp_h_samp);
        check!(comp_v_samp);
        check!(comp_quant_tbl_no);
        check!(comp_dc_tbl_no);
        check!(comp_ac_tbl_no);
        check!(optimize_scans);
        check!(use_scans_in_trellis);
        check!(dc_scan_opt_mode);
        check!(compress_profile);
        diffs
    }
}

/// Configure a Compress instance with every tracked field set to a non-default value.
///
/// CRITICAL: Every field must be set to a value DIFFERENT from both:
///   (a) the initial default after Compress::new(), and
///   (b) the value that jpeg_set_defaults() would produce.
///
/// Every field is asserted immediately after being set so we can guarantee
/// the "before" snapshot truly contains non-default values. If any assertion
/// fails, we know our test setup is broken rather than silently missing resets.
///
/// Default values after Compress::new(JCS_RGB) with mozjpeg profile:
///   smoothing_factor=0, raw_data_in=0, optimize_coding=1(TRUE),
///   density_unit=0, x_density=1, y_density=1, num_scans=64,
///   comp_h_samp=[2,1,1], comp_v_samp=[2,1,1],
///   comp_quant_tbl_no=[0,1,1], comp_dc_tbl_no=[0,1,1], comp_ac_tbl_no=[0,1,1],
///   optimize_scans=true, use_scans_in_trellis=false,
///   dc_scan_opt_mode=0, compress_profile=JCP_MAX_COMPRESSION
fn set_all_nondefault(comp: &mut Compress) {
    // Set YCbCr color space so we have 3 components to test
    comp.set_color_space(ColorSpace::JCS_YCbCr);
    comp.set_size(64, 64);

    // Quality (default after jpeg_set_defaults is 75)
    comp.set_quality(92.0);

    // smoothing_factor: default=0, set to 50
    comp.set_smoothing_factor(50);
    assert_eq!(comp.cinfo().smoothing_factor, 50, "smoothing_factor should be 50 (non-default 0)");

    // density_unit: default=0, x_density: default=1, y_density: default=1
    comp.set_pixel_density(PixelDensity {
        unit: PixelDensityUnit::Inches,
        x: 300,
        y: 300,
    });
    assert_eq!(comp.cinfo().density_unit, 1, "density_unit should be 1 (non-default 0)");
    assert_eq!(comp.cinfo().X_density, 300, "x_density should be 300 (non-default 1)");
    assert_eq!(comp.cinfo().Y_density, 300, "y_density should be 300 (non-default 1)");

    // optimize_coding: mozjpeg default=1 (TRUE).
    // Set to FALSE so we detect when jpeg_set_defaults() + mozjpeg profile flips it back to TRUE.
    comp.set_optimize_coding(false);
    assert_eq!(comp.cinfo().optimize_coding, 0, "optimize_coding should be 0/FALSE (non-default 1/TRUE)");

    // optimize_scans: mozjpeg default=true.
    // Set to FALSE so we detect when jpeg_set_defaults() + mozjpeg profile flips it back to TRUE.
    comp.set_optimize_scans(false);
    let optimize_scans = unsafe {
        mozjpeg_sys::jpeg_c_get_bool_param(
            comp.cinfo(),
            mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS,
        )
    };
    assert_eq!(optimize_scans, 0, "optimize_scans should be 0/FALSE (non-default TRUE)");

    // use_scans_in_trellis: default=false, set to true
    comp.set_use_scans_in_trellis(true);
    let use_scans = unsafe {
        mozjpeg_sys::jpeg_c_get_bool_param(
            comp.cinfo(),
            mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_USE_SCANS_IN_TRELLIS,
        )
    };
    assert_eq!(use_scans, 1, "use_scans_in_trellis should be 1/TRUE (non-default FALSE)");

    // dc_scan_opt_mode: default=0, set to 2 via unsafe FFI (no Rust API setter for testing).
    unsafe {
        mozjpeg_sys::jpeg_c_set_int_param(
            comp.cinfo_mut(),
            mozjpeg_sys::J_INT_PARAM::JINT_DC_SCAN_OPT_MODE,
            2,
        );
    }
    let dc_mode = unsafe {
        mozjpeg_sys::jpeg_c_get_int_param(
            comp.cinfo(),
            mozjpeg_sys::J_INT_PARAM::JINT_DC_SCAN_OPT_MODE,
        )
    };
    assert_eq!(dc_mode, 2, "dc_scan_opt_mode should be 2 (non-default 0)");

    // compress_profile: DO NOT CHANGE. Left at JCP_MAX_COMPRESSION (the default).
    //
    // This is intentional: compress_profile controls what jpeg_set_defaults() does.
    // Changing it to JCP_FASTEST would make jpeg_set_defaults() produce JCP_FASTEST
    // behavior, masking resets for optimize_coding/optimize_scans.
    //
    // set_fastest_defaults() explicitly changes compress_profile to JCP_FASTEST before
    // calling jpeg_set_defaults(), and that reset is tested separately.

    // raw_data_in: default=0 (false), set to true (1)
    comp.set_raw_data_in(true);
    assert_eq!(comp.cinfo().raw_data_in, 1, "raw_data_in should be 1/TRUE (non-default 0/FALSE)");

    // Component fields: sampling factors, quant/huffman table assignments
    // Defaults: h_samp=[2,1,1], v_samp=[2,1,1], quant_tbl_no=[0,1,1],
    //           dc_tbl_no=[0,1,1], ac_tbl_no=[0,1,1]
    {
        let comps = comp.components_mut();
        // Sampling: 4:2:2 instead of default 4:2:0
        comps[0].h_samp_factor = 2;
        comps[0].v_samp_factor = 1; // default 2
        comps[1].h_samp_factor = 1;
        comps[1].v_samp_factor = 1;
        comps[2].h_samp_factor = 1;
        comps[2].v_samp_factor = 1;
        // Quant table assignments: swap from default [0,1,1] to [1,0,0]
        comps[0].quant_tbl_no = 1; // default 0
        comps[1].quant_tbl_no = 0; // default 1
        comps[2].quant_tbl_no = 0; // default 1
        // DC huffman table: swap from default [0,1,1] to [1,0,0]
        comps[0].dc_tbl_no = 1; // default 0
        comps[1].dc_tbl_no = 0; // default 1
        comps[2].dc_tbl_no = 0; // default 1
        // AC huffman table: swap from default [0,1,1] to [1,0,0]
        comps[0].ac_tbl_no = 1; // default 0
        comps[1].ac_tbl_no = 0; // default 1
        comps[2].ac_tbl_no = 0; // default 1
    }
    assert_eq!(comp.components()[0].v_samp_factor, 1, "v_samp[0] should be 1 (non-default 2, 4:2:2)");
    assert_eq!(comp.components()[0].h_samp_factor, 2, "h_samp[0] should be 2");
    assert_eq!(comp.components()[0].quant_tbl_no, 1, "quant_tbl_no[0] should be 1 (non-default 0)");
    assert_eq!(comp.components()[1].quant_tbl_no, 0, "quant_tbl_no[1] should be 0 (non-default 1)");
    assert_eq!(comp.components()[0].dc_tbl_no, 1, "dc_tbl_no[0] should be 1 (non-default 0)");
    assert_eq!(comp.components()[1].dc_tbl_no, 0, "dc_tbl_no[1] should be 0 (non-default 1)");
    assert_eq!(comp.components()[0].ac_tbl_no, 1, "ac_tbl_no[0] should be 1 (non-default 0)");
    assert_eq!(comp.components()[1].ac_tbl_no, 0, "ac_tbl_no[1] should be 0 (non-default 1)");

    // num_scans: default=64 (mozjpeg progressive). This is set internally by scan info
    // manipulation and not directly controllable without risk of dangling scan_info pointers.
    // We leave it as-is and rely on the diff to catch changes (set_fastest_defaults resets
    // it to 0 by switching to JCP_FASTEST profile which disables progressive).
    // The fact that set_scan_optimization_mode() does NOT change num_scans is itself verified
    // by the diff showing no num_scans entry for that method.
}

/// Test what set_scan_optimization_mode() resets at the struct level
#[test]
fn scan_optimization_mode_resets_struct_fields() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);

    comp.set_scan_optimization_mode(ScanMode::Auto);

    let after = CinfoSnapshot::capture(&comp);
    let diffs = before.diff(&after);

    println!("\n=== Fields reset by set_scan_optimization_mode(ScanMode::Auto) ===\n");

    if diffs.is_empty() {
        println!("No fields were reset (all preserved).");
    } else {
        println!("{:<30} {:<30} {:<30}", "FIELD", "BEFORE", "AFTER");
        println!("{:-<30} {:-<30} {:-<30}", "", "", "");
        for (field, before_val, after_val) in &diffs {
            println!("{:<30} {:<30} {:<30}", field, before_val, after_val);
        }
    }

    // Document what we expect to be PRESERVED (fixed on this branch)
    assert_eq!(
        before.smoothing_factor, after.smoothing_factor,
        "smoothing_factor should be preserved (fixed)"
    );

    // Document what we expect to be RESET (known bugs, not yet fixed)
    let reset_fields: Vec<&str> = diffs.iter().map(|(f, _, _)| f.as_str()).collect();

    println!("\n=== Summary ===");
    println!("Fields RESET: {:?}", reset_fields);
    println!(
        "Fields PRESERVED: everything else ({} fields checked)",
        16 - reset_fields.len()
    );

    // Print the complete list for documentation
    println!("\n=== Complete field status ===");
    let field_names = [
        "smoothing_factor",
        "raw_data_in",
        "optimize_coding",
        "density_unit",
        "x_density",
        "y_density",
        "num_scans",
        "comp_h_samp",
        "comp_v_samp",
        "comp_quant_tbl_no",
        "comp_dc_tbl_no",
        "comp_ac_tbl_no",
        "optimize_scans",
        "use_scans_in_trellis",
        "dc_scan_opt_mode",
        "compress_profile",
    ];
    for name in &field_names {
        let was_reset = reset_fields.contains(name);
        if was_reset {
            println!("  RESET:     {}", name);
        } else {
            println!("  PRESERVED: {}", name);
        }
    }
}

/// Test what set_fastest_defaults() resets at the struct level
#[test]
fn fastest_defaults_resets_struct_fields() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);

    comp.set_fastest_defaults();

    let after = CinfoSnapshot::capture(&comp);
    let diffs = before.diff(&after);

    println!("\n=== Fields reset by set_fastest_defaults() ===\n");

    if diffs.is_empty() {
        println!("No fields were reset (all preserved).");
    } else {
        println!("{:<30} {:<30} {:<30}", "FIELD", "BEFORE", "AFTER");
        println!("{:-<30} {:-<30} {:-<30}", "", "", "");
        for (field, before_val, after_val) in &diffs {
            println!("{:<30} {:<30} {:<30}", field, before_val, after_val);
        }
    }

    // smoothing_factor should be preserved (existing fix)
    assert_eq!(
        before.smoothing_factor, after.smoothing_factor,
        "smoothing_factor should be preserved (fixed)"
    );

    let reset_fields: Vec<&str> = diffs.iter().map(|(f, _, _)| f.as_str()).collect();

    println!("\n=== Summary ===");
    println!("Fields RESET: {:?}", reset_fields);
    println!(
        "Fields PRESERVED: everything else ({} fields checked)",
        16 - reset_fields.len()
    );

    // The main diff test above uses optimize_coding=FALSE and optimize_scans=FALSE,
    // which happen to match what JCP_FASTEST produces — so the diff can't see them.
    // But set_fastest_defaults() DOES reset these if the user had them at TRUE
    // (the mozjpeg default). Verify this explicitly:
    let mut comp2 = Compress::new(ColorSpace::JCS_RGB);
    comp2.set_size(64, 64);
    // mozjpeg default: optimize_coding=TRUE, optimize_scans=TRUE
    assert_eq!(comp2.cinfo().optimize_coding, 1, "precondition: optimize_coding starts TRUE");
    let opt_scans_before = unsafe {
        mozjpeg_sys::jpeg_c_get_bool_param(
            comp2.cinfo(),
            mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS,
        )
    };
    assert_eq!(opt_scans_before, 1, "precondition: optimize_scans starts TRUE");

    comp2.set_fastest_defaults();

    assert_eq!(
        comp2.cinfo().optimize_coding, 0,
        "set_fastest_defaults() should reset optimize_coding TRUE→FALSE"
    );
    let opt_scans_after = unsafe {
        mozjpeg_sys::jpeg_c_get_bool_param(
            comp2.cinfo(),
            mozjpeg_sys::J_BOOLEAN_PARAM::JBOOLEAN_OPTIMIZE_SCANS,
        )
    };
    assert_eq!(
        opt_scans_after, 0,
        "set_fastest_defaults() should reset optimize_scans TRUE→FALSE"
    );
    println!("  (Also verified: optimize_coding TRUE→FALSE, optimize_scans TRUE→FALSE)");
}

/// Test each ScanMode variant to see if they reset differently
#[test]
fn scan_mode_variants_reset_comparison() {
    let modes = [
        ("AllComponentsTogether", ScanMode::AllComponentsTogether),
        ("ScanPerComponent", ScanMode::ScanPerComponent),
        ("Auto", ScanMode::Auto),
    ];

    println!("\n=== Comparing resets across ScanMode variants ===\n");

    for (name, mode) in &modes {
        let mut comp = Compress::new(ColorSpace::JCS_RGB);
        set_all_nondefault(&mut comp);

        let before = CinfoSnapshot::capture(&comp);
        comp.set_scan_optimization_mode(*mode);
        let after = CinfoSnapshot::capture(&comp);

        let diffs = before.diff(&after);
        let reset_fields: Vec<&str> = diffs.iter().map(|(f, _, _)| f.as_str()).collect();

        println!("ScanMode::{}: {} fields reset", name, reset_fields.len());
        for (field, before_val, after_val) in &diffs {
            println!("  {} : {} → {}", field, before_val, after_val);
        }
        println!();
    }
}

/// Verify that calling set_scan_optimization_mode() twice doesn't compound resets
#[test]
fn double_call_idempotent() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(64, 64);
    comp.set_quality(85.0);
    comp.set_smoothing_factor(50);

    comp.set_scan_optimization_mode(ScanMode::Auto);
    let after_first = CinfoSnapshot::capture(&comp);

    comp.set_scan_optimization_mode(ScanMode::Auto);
    let after_second = CinfoSnapshot::capture(&comp);

    let diffs = after_first.diff(&after_second);
    assert!(
        diffs.is_empty(),
        "Calling set_scan_optimization_mode() twice should be idempotent, but these fields changed: {:?}",
        diffs
    );
}

/// Test raw_data_in specifically - previous analysis claimed it was reset,
/// but the struct-level test above didn't show that. This tests it explicitly.
#[test]
fn raw_data_in_reset_behavior() {
    println!("\n=== raw_data_in reset behavior ===\n");

    // Test 1: raw_data_in BEFORE set_scan_optimization_mode
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(64, 64);
    comp.set_raw_data_in(true);
    assert_eq!(comp.cinfo().raw_data_in, 1, "raw_data_in should be set");

    comp.set_scan_optimization_mode(ScanMode::Auto);
    println!(
        "raw_data_in after set_scan_optimization_mode: {}",
        comp.cinfo().raw_data_in
    );

    // Test 2: raw_data_in BEFORE set_fastest_defaults
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(64, 64);
    comp.set_raw_data_in(true);
    assert_eq!(comp.cinfo().raw_data_in, 1, "raw_data_in should be set");

    comp.set_fastest_defaults();
    println!(
        "raw_data_in after set_fastest_defaults: {}",
        comp.cinfo().raw_data_in
    );
}

/// Test what set_color_space() resets.
/// It calls jpeg_set_colorspace() which uses the SET_COMP macro to reset component fields.
#[test]
fn set_color_space_resets() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_color_space(ColorSpace::JCS_YCbCr);
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_color_space(JCS_YCbCr) ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        println!("{:<30} {:<30} {:<30}", "FIELD", "BEFORE", "AFTER");
        println!("{:-<30} {:-<30} {:-<30}", "", "", "");
        for (field, before_val, after_val) in &diffs {
            println!("{:<30} {:<30} {:<30}", field, before_val, after_val);
        }
    }
}

/// Test what set_quality() resets.
/// It calls jpeg_set_quality() which regenerates quantization tables.
#[test]
fn set_quality_resets() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_quality(50.0);
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_quality(50.0) ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        println!("{:<30} {:<30} {:<30}", "FIELD", "BEFORE", "AFTER");
        println!("{:-<30} {:-<30} {:-<30}", "", "", "");
        for (field, before_val, after_val) in &diffs {
            println!("{:<30} {:<30} {:<30}", field, before_val, after_val);
        }
    }
}

/// Test what set_progressive_mode() resets.
/// It calls jpeg_simple_progression().
#[test]
fn set_progressive_mode_resets() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_progressive_mode();
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_progressive_mode() ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        println!("{:<30} {:<30} {:<30}", "FIELD", "BEFORE", "AFTER");
        println!("{:-<30} {:-<30} {:-<30}", "", "", "");
        for (field, before_val, after_val) in &diffs {
            println!("{:<30} {:<30} {:<30}", field, before_val, after_val);
        }
    }
}

/// Test what set_optimize_scans() resets.
/// Uses jpeg_c_set_bool_param.
#[test]
fn set_optimize_scans_resets() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_optimize_scans(true);
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_optimize_scans(true) ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        for (field, before_val, after_val) in &diffs {
            println!("  {} : {} → {}", field, before_val, after_val);
        }
    }
}

/// Test what set_luma_qtable() and set_chroma_qtable() reset.
#[test]
fn set_qtable_resets() {
    use mozjpeg::qtable::Flat;

    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_luma_qtable(&Flat.scaled(50.0, 50.0));
    comp.set_chroma_qtable(&Flat.scaled(80.0, 80.0));
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_luma_qtable + set_chroma_qtable ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        for (field, before_val, after_val) in &diffs {
            println!("  {} : {} → {}", field, before_val, after_val);
        }
    }
}

/// Test what set_chroma_sampling_pixel_sizes() resets (the safe subsampling API).
#[test]
fn set_chroma_sampling_resets() {
    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    set_all_nondefault(&mut comp);

    let before = CinfoSnapshot::capture(&comp);
    comp.set_chroma_sampling_pixel_sizes((2, 1), (2, 1)); // 4:2:2
    let after = CinfoSnapshot::capture(&comp);

    let diffs = before.diff(&after);
    println!("\n=== Fields reset by set_chroma_sampling_pixel_sizes((2,1),(2,1)) ===\n");
    if diffs.is_empty() {
        println!("No fields were reset.");
    } else {
        for (field, before_val, after_val) in &diffs {
            println!("  {} : {} → {}", field, before_val, after_val);
        }
    }
}

/// Comprehensive: set every method, snapshot after each, report any unexpected side effects.
/// This traces the full public API method by method.
#[test]
fn trace_every_public_method() {
    println!("\n=== Tracing all public Compress methods for side effects ===\n");

    type MethodEntry<'a> = (&'a str, Box<dyn Fn(&mut Compress)>);
    let methods: Vec<MethodEntry<'_>> = vec![
        ("set_size(64, 64)", Box::new(|c: &mut Compress| c.set_size(64, 64))),
        ("set_color_space(YCbCr)", Box::new(|c: &mut Compress| c.set_color_space(ColorSpace::JCS_YCbCr))),
        ("set_quality(85.0)", Box::new(|c: &mut Compress| c.set_quality(85.0))),
        ("set_smoothing_factor(50)", Box::new(|c: &mut Compress| c.set_smoothing_factor(50))),
        ("set_pixel_density(300dpi)", Box::new(|c: &mut Compress| {
            c.set_pixel_density(PixelDensity {
                unit: PixelDensityUnit::Inches,
                x: 300,
                y: 300,
            });
        })),
        ("set_optimize_coding(true)", Box::new(|c: &mut Compress| c.set_optimize_coding(true))),
        ("set_progressive_mode()", Box::new(|c: &mut Compress| c.set_progressive_mode())),
        ("set_optimize_scans(true)", Box::new(|c: &mut Compress| c.set_optimize_scans(true))),
        ("set_use_scans_in_trellis(true)", Box::new(|c: &mut Compress| c.set_use_scans_in_trellis(true))),
        ("set_raw_data_in(true)", Box::new(|c: &mut Compress| c.set_raw_data_in(true))),
        ("set_scan_optimization_mode(Auto)", Box::new(|c: &mut Compress| c.set_scan_optimization_mode(ScanMode::Auto))),
        ("set_fastest_defaults()", Box::new(|c: &mut Compress| c.set_fastest_defaults())),
        ("set_chroma_sampling_pixel_sizes(4:2:2)", Box::new(|c: &mut Compress| c.set_chroma_sampling_pixel_sizes((2, 1), (2, 1)))),
        ("components_mut() 4:2:2", Box::new(|c: &mut Compress| {
            let comps = c.components_mut();
            if comps.len() >= 3 {
                comps[0].h_samp_factor = 2;
                comps[0].v_samp_factor = 1;
                comps[1].h_samp_factor = 1;
                comps[1].v_samp_factor = 1;
                comps[2].h_samp_factor = 1;
                comps[2].v_samp_factor = 1;
            }
        })),
    ];

    for (name, method) in &methods {
        // Start fresh each time with non-default values
        let mut comp = Compress::new(ColorSpace::JCS_RGB);
        set_all_nondefault(&mut comp);

        let before = CinfoSnapshot::capture(&comp);
        method(&mut comp);
        let after = CinfoSnapshot::capture(&comp);

        let diffs = before.diff(&after);
        if diffs.is_empty() {
            println!("  {:<45} no side effects", name);
        } else {
            let fields: Vec<&str> = diffs.iter().map(|(f, _, _)| f.as_str()).collect();
            println!("  {:<45} RESETS: {:?}", name, fields);
        }
    }
}

/// Check the default values that jpeg_set_defaults() establishes in Compress::new()
#[test]
fn document_defaults_after_new() {
    let comp = Compress::new(ColorSpace::JCS_RGB);
    let snap = CinfoSnapshot::capture(&comp);

    println!("\n=== Default values after Compress::new(JCS_RGB) ===\n");
    println!("smoothing_factor:     {}", snap.smoothing_factor);
    println!("raw_data_in:          {}", snap.raw_data_in);
    println!("optimize_coding:      {}", snap.optimize_coding);
    println!("density_unit:         {}", snap.density_unit);
    println!("x_density:            {}", snap.x_density);
    println!("y_density:            {}", snap.y_density);
    println!("num_scans:            {}", snap.num_scans);
    println!("comp_h_samp:          {:?}", snap.comp_h_samp);
    println!("comp_v_samp:          {:?}", snap.comp_v_samp);
    println!("comp_quant_tbl_no:    {:?}", snap.comp_quant_tbl_no);
    println!("comp_dc_tbl_no:       {:?}", snap.comp_dc_tbl_no);
    println!("comp_ac_tbl_no:       {:?}", snap.comp_ac_tbl_no);
    println!("optimize_scans:       {}", snap.optimize_scans);
    println!("use_scans_in_trellis: {}", snap.use_scans_in_trellis);
    println!("dc_scan_opt_mode:     {}", snap.dc_scan_opt_mode);
    println!("compress_profile:     {}", snap.compress_profile);
}


/// Verify that jpeg_set_defaults() overwrites quantization table CONTENTS
/// (not just the component→table slot assignments).
#[test]
fn qtable_contents_reset_by_defaults() {
    use mozjpeg::qtable::Flat;

    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_color_space(ColorSpace::JCS_YCbCr);
    comp.set_size(64, 64);

    // Write custom flat tables (all 50s) to slots 0 and 1
    comp.set_luma_qtable(&Flat.scaled(50.0, 50.0));
    comp.set_chroma_qtable(&Flat.scaled(80.0, 80.0));

    // Read back table contents from slot 0
    let tbl0_before = unsafe {
        let tbl_ptr = (*comp.cinfo()).quant_tbl_ptrs[0];
        assert!(!tbl_ptr.is_null(), "quant table 0 should exist");
        (*tbl_ptr).quantval
    };
    println!("Slot 0 before: first 8 coeffs = {:?}", &tbl0_before[..8]);

    let tbl1_before = unsafe {
        let tbl_ptr = (*comp.cinfo()).quant_tbl_ptrs[1];
        assert!(!tbl_ptr.is_null(), "quant table 1 should exist");
        (*tbl_ptr).quantval
    };
    println!("Slot 1 before: first 8 coeffs = {:?}", &tbl1_before[..8]);

    // Now call set_scan_optimization_mode which calls jpeg_set_defaults
    comp.set_scan_optimization_mode(ScanMode::Auto);

    let tbl0_after = unsafe {
        let tbl_ptr = (*comp.cinfo()).quant_tbl_ptrs[0];
        assert!(!tbl_ptr.is_null(), "quant table 0 should still exist");
        (*tbl_ptr).quantval
    };
    println!("Slot 0 after:  first 8 coeffs = {:?}", &tbl0_after[..8]);

    let tbl1_after = unsafe {
        let tbl_ptr = (*comp.cinfo()).quant_tbl_ptrs[1];
        assert!(!tbl_ptr.is_null(), "quant table 1 should still exist");
        (*tbl_ptr).quantval
    };
    println!("Slot 1 after:  first 8 coeffs = {:?}", &tbl1_after[..8]);

    let slot0_changed = tbl0_before != tbl0_after;
    let slot1_changed = tbl1_before != tbl1_after;
    println!("\nSlot 0 contents changed: {}", slot0_changed);
    println!("Slot 1 contents changed: {}", slot1_changed);

    // Also check: do the component assignments still point to the right slots?
    let comps = comp.components();
    println!("\nComponent table assignments after reset:");
    for (i, c) in comps.iter().enumerate().take(3) {
        println!("  comp[{}]: quant_tbl_no={}, dc_tbl_no={}, ac_tbl_no={}",
            i, c.quant_tbl_no, c.dc_tbl_no, c.ac_tbl_no);
    }
}
