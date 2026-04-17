//! FCPXML 1.11 exporter for Final Cut Pro.
//!
//! Everything uses rational time (`"numerator/denominator s"`) because
//! Final Cut is strict about timebase math. The denominator is picked per
//! frame rate from `frame_duration_for_fps`.

use crate::writer::XmlBuilder;
use crate::NleExportInput;

/// Build the FCPXML bytes for the given input. Errors when the keep list is
/// empty — an FCPXML project without clips is technically valid but almost
/// certainly a user mistake, so we surface it.
pub fn build_fcpxml(input: &NleExportInput) -> Result<Vec<u8>, String> {
    if input.keep_ranges_ms.is_empty() {
        return Err("keep_ranges_ms is empty".into());
    }

    let (frame_duration, denom) = frame_duration_for_fps(input.frame_rate);
    let tc_format = tc_format_for_fps(input.frame_rate);
    let format_name = format_name_for(input.height, input.frame_rate);
    let source_url = file_url(&input.source_path);

    let asset_duration = rational_ms(input.total_duration_ms, denom);
    let sequence_duration_ms: i64 = input.keep_ranges_ms.iter().map(|(s, e)| e - s).sum();
    let sequence_duration = rational_ms(sequence_duration_ms, denom);

    let mut b = XmlBuilder::new(true);
    b.declaration();
    b.doctype("fcpxml");

    b.open("fcpxml", &[("version", "1.11".to_string())]);

    // <resources>
    b.open("resources", &[]);
    b.empty(
        "format",
        &[
            ("id", "r1".to_string()),
            ("name", format_name.clone()),
            ("frameDuration", frame_duration.clone()),
            ("width", input.width.to_string()),
            ("height", input.height.to_string()),
        ],
    );
    b.open(
        "asset",
        &[
            ("id", "r2".to_string()),
            ("name", input.asset_name.clone()),
            ("start", "0s".to_string()),
            ("duration", asset_duration.clone()),
            ("hasVideo", "1".to_string()),
            ("format", "r1".to_string()),
            ("hasAudio", "1".to_string()),
            ("videoSources", "1".to_string()),
            ("audioSources", "1".to_string()),
            ("audioChannels", input.audio_channels.to_string()),
        ],
    );
    b.empty(
        "media-rep",
        &[
            ("kind", "original-media".to_string()),
            ("src", source_url),
        ],
    );
    b.close("asset");
    b.close("resources");

    // <library><event><project><sequence><spine>
    b.open("library", &[]);
    b.open("event", &[("name", input.project_name.clone())]);
    b.open("project", &[("name", input.project_name.clone())]);
    b.open(
        "sequence",
        &[
            ("format", "r1".to_string()),
            ("duration", sequence_duration),
            ("tcStart", "0s".to_string()),
            ("tcFormat", tc_format.clone()),
        ],
    );
    b.open("spine", &[]);

    // One <asset-clip> per keep region.
    let mut cursor_ms: i64 = 0;
    for (start_ms, end_ms) in &input.keep_ranges_ms {
        let clip_duration_ms = end_ms - start_ms;
        b.empty(
            "asset-clip",
            &[
                ("ref", "r2".to_string()),
                ("offset", rational_ms(cursor_ms, denom)),
                ("start", rational_ms(*start_ms, denom)),
                ("duration", rational_ms(clip_duration_ms, denom)),
                ("format", "r1".to_string()),
                ("tcFormat", tc_format.clone()),
                ("audioRole", "dialogue".to_string()),
            ],
        );
        cursor_ms += clip_duration_ms;
    }

    b.close("spine");
    b.close("sequence");
    b.close("project");
    b.close("event");
    b.close("library");
    b.close("fcpxml");

    Ok(b.into_bytes())
}

/// Convert a millisecond value into FCPXML rational `"num/denom s"` form.
pub fn rational_ms(ms: i64, denom: i64) -> String {
    let num = (ms * denom) / 1000;
    format!("{num}/{denom}s")
}

/// Map a frame rate to an FCPXML-accepted `frameDuration` and its
/// denominator (used for all other rational fields in the same sequence).
pub fn frame_duration_for_fps(fps: f64) -> (String, i64) {
    let (num, denom): (i64, i64) = match fps_bucket(fps) {
        FpsBucket::Fps23_976 => (1001, 24_000),
        FpsBucket::Fps24 => (100, 2400),
        FpsBucket::Fps25 => (1, 25),
        FpsBucket::Fps29_97 => (1001, 30_000),
        FpsBucket::Fps30 => (100, 3000),
        FpsBucket::Fps50 => (1, 50),
        FpsBucket::Fps59_94 => (1001, 60_000),
        FpsBucket::Fps60 => (100, 6000),
    };
    let shared_denom = match (num, denom) {
        (_, 24_000) => 24_000,
        (_, 30_000) => 30_000,
        (_, 60_000) => 60_000,
        _ => 30_000,
    };
    (format!("{num}/{denom}s"), shared_denom)
}

#[derive(Debug, Clone, Copy)]
enum FpsBucket {
    Fps23_976,
    Fps24,
    Fps25,
    Fps29_97,
    Fps30,
    Fps50,
    Fps59_94,
    Fps60,
}

fn fps_bucket(fps: f64) -> FpsBucket {
    // Snap to the nearest standard rate so human-entered values like "29.97"
    // and "29.970" both land on the NTSC bucket.
    let candidates = [
        (23.976, FpsBucket::Fps23_976),
        (24.0, FpsBucket::Fps24),
        (25.0, FpsBucket::Fps25),
        (29.97, FpsBucket::Fps29_97),
        (30.0, FpsBucket::Fps30),
        (50.0, FpsBucket::Fps50),
        (59.94, FpsBucket::Fps59_94),
        (60.0, FpsBucket::Fps60),
    ];
    candidates
        .iter()
        .min_by(|(a, _), (b, _)| {
            (a - fps)
                .abs()
                .partial_cmp(&(b - fps).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, b)| *b)
        .unwrap_or(FpsBucket::Fps30)
}

pub fn tc_format_for_fps(fps: f64) -> String {
    match fps_bucket(fps) {
        FpsBucket::Fps29_97 | FpsBucket::Fps59_94 => "DF".into(),
        _ => "NDF".into(),
    }
}

pub fn format_name_for(height: u32, fps: f64) -> String {
    let rounded = match fps_bucket(fps) {
        FpsBucket::Fps23_976 => "2398",
        FpsBucket::Fps24 => "24",
        FpsBucket::Fps25 => "25",
        FpsBucket::Fps29_97 => "2997",
        FpsBucket::Fps30 => "30",
        FpsBucket::Fps50 => "50",
        FpsBucket::Fps59_94 => "5994",
        FpsBucket::Fps60 => "60",
    };
    format!("FFVideoFormat{height}p{rounded}")
}

/// Convert an absolute path to a `file://` URL that FCP will resolve.
/// `Path::display` is enough for macOS; Windows paths need backslashes
/// flipped and a `/` prefix.
pub fn file_url(path: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        let s = path.display().to_string().replace('\\', "/");
        if s.starts_with('/') {
            format!("file://{s}")
        } else {
            format!("file:///{s}")
        }
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_input() -> NleExportInput {
        NleExportInput {
            source_path: PathBuf::from("/Users/demo/video.mov"),
            asset_name: "video.mov".into(),
            project_name: "My Media Kit Export".into(),
            total_duration_ms: 10_000,
            frame_rate: 30.0,
            width: 1920,
            height: 1080,
            audio_channels: 2,
            keep_ranges_ms: vec![(0, 3_000), (5_000, 8_000)],
        }
    }

    #[test]
    fn rational_produces_expected_form() {
        assert_eq!(rational_ms(2073, 30_000), "62190/30000s");
    }

    #[test]
    fn frame_duration_maps_standard_rates() {
        assert_eq!(frame_duration_for_fps(29.97).0, "1001/30000s");
        assert_eq!(frame_duration_for_fps(30.0).0, "100/3000s");
        assert_eq!(frame_duration_for_fps(23.976).0, "1001/24000s");
    }

    #[test]
    fn format_name_matches_convention() {
        assert_eq!(format_name_for(1080, 30.0), "FFVideoFormat1080p30");
        assert_eq!(format_name_for(2160, 29.97), "FFVideoFormat2160p2997");
    }

    #[test]
    fn fcpxml_contains_required_structure() {
        let bytes = build_fcpxml(&sample_input()).unwrap();
        let xml = String::from_utf8(bytes).unwrap();
        assert!(xml.contains("<fcpxml version=\"1.11\">"));
        assert!(xml.contains("<format id=\"r1\""));
        assert!(xml.contains("<asset id=\"r2\""));
        assert!(xml.contains("<media-rep"));
        assert!(xml.contains("<asset-clip"));
        assert!(xml.contains("</fcpxml>"));
    }

    #[test]
    fn fcpxml_emits_one_clip_per_keep_region() {
        let bytes = build_fcpxml(&sample_input()).unwrap();
        let xml = String::from_utf8(bytes).unwrap();
        let clips = xml.matches("<asset-clip").count();
        assert_eq!(clips, 2);
    }

    #[test]
    fn fcpxml_empty_keeps_error() {
        let mut input = sample_input();
        input.keep_ranges_ms.clear();
        assert!(build_fcpxml(&input).is_err());
    }

    #[test]
    fn file_url_has_file_scheme() {
        let url = file_url(std::path::Path::new("/tmp/x.mov"));
        assert!(url.starts_with("file://"));
        assert!(url.ends_with("/tmp/x.mov"));
    }
}
