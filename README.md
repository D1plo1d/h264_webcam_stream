## h264_webcam_stream

This crate provides h264 video streams from any v4l2 video device.

Video devices that only support mjpeg are re-encoded as h264 by the openh264 library (A C++ encoder which should work on any CPU architectures).

### Capturing H264 Video

Streaming an h264 file to video is simple:

```rust
fn main {
    let device_path = Path::new("/dev/video0");
    let max_fps = 60;

    let mut device = h264_webcam_stream::get_device(&device_path)?;
    let mut stream = h264_webcam_stream::stream(&mut device, max_fps)?;

    let mut file = std::fs::File::create("./test.h264")?;

    for 0...120 {
        let (h264_bytes, _) = stream.next(false)?;
        // Write the timelapse h264 output to file
        f.write_all(&h264_bytes[..])?;
    }
}
```

### Listing Video Capture Devices

Getting a list of the video capture devices is also easy:

```rust
let devices: Vec<_> = h264_webcam_stream::list_devices()
    .into_iter()
    .collect();
```

### Capturing Still Images

h264_webcam_stream supports capturing YUV-encoded images at the same time as the H264 video stream.

The YUV image capture can be useful for capturing still images from the video feed or for creating a timelapse video by selectively re-encoding frames as h264 at a fixed interval or based on some external trigger.

To enable image capture along side video capture, pass true to `stream.next`:

```rust
let (h264_bytes, yuv_still_image) = stream.next(true)?;
```

### Linux Only

This crate only supports Linux for the time being.

I have no plans to implement support for other operating systems myself but if you would like to implement h264 webcam streaming for another OS please feel welcome to submit a pull request!
