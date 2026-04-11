//! xmeml v5 exporter — consumed by Adobe Premiere Pro and DaVinci Resolve.
//!
//! Unlike FCPXML, xmeml counts everything in integer frames (not rational
//! time). `<rate>`, `<timebase>`, `<ntsc>` describe the sequence rate and
//! `<in>`/`<out>` on each clipitem are frame offsets within the source media.
//!
//! Based on the Swift template reverse-engineered from
//! `docs/06-nle-export.md` + `Resources/sample_adobe_premier.xml`.

use crate::writer::XmlBuilder;
use crate::NleExportInput;

pub fn build_xmeml(input: &NleExportInput) -> Result<Vec<u8>, String> {
    if input.keep_ranges_ms.is_empty() {
        return Err("keep_ranges_ms is empty".into());
    }

    let (timebase, ntsc) = timebase_and_ntsc(input.frame_rate);
    let fps_effective = effective_fps(input.frame_rate);
    let total_frames = ms_to_frames(input.total_duration_ms, fps_effective);
    let sequence_duration_ms: i64 = input.keep_ranges_ms.iter().map(|(s, e)| e - s).sum();
    let sequence_frames = ms_to_frames(sequence_duration_ms, fps_effective);

    let mut b = XmlBuilder::new(true);
    b.declaration();
    b.doctype("xmeml");
    b.open("xmeml", &[("version", "5".to_string())]);
    b.open("sequence", &[]);

    b.text_element("name", &input.project_name);
    b.text_element("duration", &sequence_frames.to_string());

    // Rate
    b.open("rate", &[]);
    b.text_element("timebase", &timebase.to_string());
    b.text_element("ntsc", if ntsc { "TRUE" } else { "FALSE" });
    b.close("rate");

    // Media
    b.open("media", &[]);
    b.open("video", &[]);
    b.open("track", &[]);

    let mut cursor_frames: i64 = 0;
    for (idx, (start_ms, end_ms)) in input.keep_ranges_ms.iter().enumerate() {
        let in_frame = ms_to_frames(*start_ms, fps_effective);
        let out_frame = ms_to_frames(*end_ms, fps_effective);
        let clip_frames = out_frame - in_frame;
        let clip_start = cursor_frames;
        let clip_end = cursor_frames + clip_frames;

        b.open("clipitem", &[("id", format!("clip-{}", idx + 1))]);
        b.text_element("name", &input.asset_name);
        b.text_element("duration", &total_frames.to_string());

        b.open("rate", &[]);
        b.text_element("timebase", &timebase.to_string());
        b.text_element("ntsc", if ntsc { "TRUE" } else { "FALSE" });
        b.close("rate");

        b.text_element("start", &clip_start.to_string());
        b.text_element("end", &clip_end.to_string());
        b.text_element("in", &in_frame.to_string());
        b.text_element("out", &out_frame.to_string());

        // <file id="file-1"> on first clip, reference afterward.
        b.open(
            "file",
            &[(
                "id",
                if idx == 0 {
                    "file-1".to_string()
                } else {
                    "file-1".to_string() // reference only
                },
            )],
        );
        if idx == 0 {
            b.text_element("name", &input.asset_name);
            b.text_element("pathurl", &crate::fcpxml::file_url(&input.source_path));
            b.text_element("duration", &total_frames.to_string());
            b.open("rate", &[]);
            b.text_element("timebase", &timebase.to_string());
            b.text_element("ntsc", if ntsc { "TRUE" } else { "FALSE" });
            b.close("rate");
        }
        b.close("file");

        b.close("clipitem");
        cursor_frames += clip_frames;
    }

    b.close("track");
    b.close("video");
    b.close("media");
    b.close("sequence");
    b.close("xmeml");

    Ok(b.into_bytes())
}

pub fn timebase_and_ntsc(fps: f64) -> (i64, bool) {
    // Premiere/Resolve convention: timebase is the rounded nominal rate,
    // `ntsc=TRUE` tags fractional NTSC rates. Use a tight epsilon so integer
    // rates (30.0, 60.0) do not get snapped into the NTSC bucket.
    const EPS: f64 = 0.01;
    if (fps - 23.976).abs() < EPS {
        (24, true)
    } else if (fps - 29.97).abs() < EPS {
        (30, true)
    } else if (fps - 59.94).abs() < EPS {
        (60, true)
    } else {
        (fps.round() as i64, false)
    }
}

pub fn effective_fps(fps: f64) -> f64 {
    // Use the actual rate (23.976 / 29.97 / 59.94) for ms→frame math so
    // the clip positions stay aligned with the source media.
    fps
}

pub fn ms_to_frames(ms: i64, fps: f64) -> i64 {
    ((ms as f64 / 1000.0) * fps).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_input() -> NleExportInput {
        NleExportInput {
            source_path: PathBuf::from("/Users/demo/video.mov"),
            asset_name: "video.mov".into(),
            project_name: "CreatorUtils Export".into(),
            total_duration_ms: 10_000,
            frame_rate: 30.0,
            width: 1920,
            height: 1080,
            audio_channels: 2,
            keep_ranges_ms: vec![(0, 3_000), (5_000, 8_000)],
        }
    }

    #[test]
    fn timebase_marks_ntsc_rates() {
        assert_eq!(timebase_and_ntsc(29.97), (30, true));
        assert_eq!(timebase_and_ntsc(30.0), (30, false));
        assert_eq!(timebase_and_ntsc(23.976), (24, true));
    }

    #[test]
    fn ms_to_frames_is_round_trip_safe() {
        assert_eq!(ms_to_frames(1_000, 30.0), 30);
        assert_eq!(ms_to_frames(2_073, 30.0), 62); // 62.19 → 62
    }

    #[test]
    fn xmeml_contains_required_elements() {
        let bytes = build_xmeml(&sample_input()).unwrap();
        let xml = String::from_utf8(bytes).unwrap();
        assert!(xml.contains("<xmeml version=\"5\">"));
        assert!(xml.contains("<timebase>30</timebase>"));
        assert!(xml.contains("<ntsc>FALSE</ntsc>"));
        assert!(xml.contains("<clipitem id=\"clip-1\""));
        assert!(xml.contains("<clipitem id=\"clip-2\""));
        assert!(xml.contains("<pathurl>file:///Users/demo/video.mov</pathurl>") || xml.contains("<pathurl>file:///"));
    }

    #[test]
    fn xmeml_empty_keeps_error() {
        let mut input = sample_input();
        input.keep_ranges_ms.clear();
        assert!(build_xmeml(&input).is_err());
    }
}
