//! Real-media NLE export smoke test. Runs silence detection on a real file
//! then generates both FCPXML and xmeml from the resulting keep ranges and
//! verifies the XML parses back without errors.

use std::path::PathBuf;

use creator_core::{NleExportTarget, SilenceDetectorConfig};
use nle_kit::{build_fcpxml, build_xmeml, NleExportInput};
use silence_kit::{detect_silence, invert_regions};

fn test_media_path() -> Option<PathBuf> {
    std::env::var("CREATOR_UTILS_TEST_MEDIA")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

fn build_input(path: PathBuf, duration_ms: i64, keep_ranges: Vec<(i64, i64)>) -> NleExportInput {
    NleExportInput {
        source_path: path.clone(),
        asset_name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("clip")
            .to_string(),
        project_name: "CreatorUtils Smoke".into(),
        total_duration_ms: duration_ms,
        frame_rate: 30.0,
        width: 1920,
        height: 1080,
        audio_channels: 2,
        keep_ranges_ms: keep_ranges,
    }
}

/// Round-trip a generated XML string through a minimal well-formedness
/// check by re-parsing it with quick-xml. Doesn't validate against a DTD
/// — we just confirm the file is parseable and the element counts look
/// right.
fn assert_well_formed_with_elements(xml: &[u8], required_tags: &[&str]) {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut seen = std::collections::HashSet::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if let Ok(name) = std::str::from_utf8(e.name().as_ref()) {
                    seen.insert(name.to_string());
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("xml parse error at {}: {}", reader.buffer_position(), e),
        }
        buf.clear();
    }

    for &req in required_tags {
        assert!(
            seen.contains(req),
            "expected element <{req}> not present (seen: {:?})",
            seen
        );
    }
}

#[tokio::test]
async fn silence_detect_to_fcpxml_and_xmeml() {
    let Some(path) = test_media_path() else {
        eprintln!("skipped: CREATOR_UTILS_TEST_MEDIA not set");
        return;
    };

    let probe = media_kit::probe_media(&path).await.expect("probe");
    let samples = media_kit::extract_pcm_samples(&path).await.expect("pcm");

    let cfg = SilenceDetectorConfig::default();
    let result = detect_silence(&samples, &cfg, None);

    let keeps = invert_regions(&result.regions, probe.duration_ms);
    assert!(!keeps.is_empty(), "keep list should never be empty");

    println!(
        "silence regions: {}, keep ranges: {}",
        result.regions.len(),
        keeps.len()
    );
    for (i, (s, e)) in keeps.iter().enumerate() {
        println!("  keep {i}: {s}..{e} ({} ms)", e - s);
    }

    let input = build_input(path.clone(), probe.duration_ms, keeps);

    let fcpxml = build_fcpxml(&input).expect("fcpxml");
    println!("fcpxml size: {} bytes", fcpxml.len());
    assert_well_formed_with_elements(
        &fcpxml,
        &[
            "fcpxml",
            "resources",
            "format",
            "asset",
            "media-rep",
            "library",
            "event",
            "project",
            "sequence",
            "spine",
            "asset-clip",
        ],
    );

    let xmeml = build_xmeml(&input).expect("xmeml");
    println!("xmeml size: {} bytes", xmeml.len());
    assert_well_formed_with_elements(
        &xmeml,
        &[
            "xmeml", "sequence", "name", "duration", "rate", "timebase", "ntsc", "media",
            "video", "track", "clipitem", "in", "out", "file", "pathurl",
        ],
    );

    // The build_project dispatcher should return the same bytes as the
    // direct call — spot check.
    let via_dispatch =
        nle_kit::build_project(&input, NleExportTarget::FinalCutPro).expect("dispatch fcp");
    assert_eq!(via_dispatch.len(), fcpxml.len());
}
