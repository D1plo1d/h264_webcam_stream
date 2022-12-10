use eyre::Result;

fn main() -> Result<()> {
    let devices: Vec<_> = h264_webcam_stream::list_devices().into_iter().collect();

    println!("Video devices: {:?}", devices);

    Ok(())
}
