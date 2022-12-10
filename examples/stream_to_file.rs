use eyre::Result;
use std::{io::Write, path::Path};

fn main() -> Result<()> {
    let device_path = Path::new("/dev/video0");
    let max_fps = 60;

    let mut device = h264_webcam_stream::get_device(&device_path)?;
    let mut stream = h264_webcam_stream::stream(&mut device, max_fps)?;

    let mut f = std::fs::File::create("./test.h264")?;

    for _ in 0..120 {
        let (h264_bytes, _) = stream.next(false)?;
        // Record the h264 video to a file
        f.write_all(&h264_bytes[..])?;
    }

    Ok(())
}
