use chrono::{Duration, Utc};
use eyre::Result;
use std::{io::Write, path::Path};

/// Record both a normal playback speed / realtime video and a timelapse that only captures 1 frame every X milliseconds.
fn main() -> Result<()> {
    let device_path = Path::new("/dev/video0");
    let max_fps = 60;

    let mut device = h264_webcam_stream::get_device(&device_path)?;
    let mut stream = h264_webcam_stream::stream(&mut device, max_fps)?;

    let mut realtime_out = std::fs::File::create("./realtime.h264")?;
    let mut timelapse_out = std::fs::File::create("./timelapse.h264")?;

    // Set up an encoder for re-encoding the timelapse video
    let h264_encoder_config = openh264::encoder::EncoderConfig::new(stream.width, stream.height);
    let mut timelapse_encoder = openh264::encoder::Encoder::with_config(h264_encoder_config)?;

    // Record a timelapse frame every X milliseconds
    let timelapse_frame_period = Duration::milliseconds(500);
    let mut last_timelapse_frame = Utc::now() - timelapse_frame_period;

    for _ in 0..240 {
        // Pass true to next to capture a still image
        let (h264_bytes, yuv_frame) = stream.next(true)?;

        // Record the realtime video to it's file
        realtime_out.write_all(&h264_bytes[..])?;

        // Add a frame to the timelapse video every X seconds when a frame is present in the h264 video feed
        let now = Utc::now();
        if let Some(yuv_frame) = yuv_frame {
            if last_timelapse_frame + timelapse_frame_period <= now {
                // Update the timestamp
                last_timelapse_frame = now;
                // Add the frame to the timelapse's h264 encoder
                let timelapse_h264_bytes = yuv_frame.encode_using(&mut timelapse_encoder)?.to_vec();
                // Record the timelapse video to it's file
                timelapse_out.write_all(&timelapse_h264_bytes[..])?;
            }
        }
    }

    Ok(())
}
