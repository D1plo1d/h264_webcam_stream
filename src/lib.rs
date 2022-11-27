pub use openh264;
pub use openh264::decoder::DecodedYUV;
use openh264::encoder::EncodedBitStream;
pub use openh264::formats::YUVBuffer;
use std::path::Path;
use std::path::PathBuf;
use thiserror::Error;
use tracing::warn;
use v4l::buffer::Type;
use v4l::frameinterval::FrameIntervalEnum;
use v4l::frameinterval::Stepwise;
use v4l::io::traits::CaptureStream;
use v4l::prelude::MmapStream;
use v4l::video::capture::Parameters;
use v4l::video::Capture;
pub use v4l::Device;
use v4l::Format;
use v4l::FourCC;

#[derive(Error, Debug)]
pub enum DeviceError {
    #[error("Video capture device failed to open")]
    CouldNotOpen(#[from] std::io::Error),
    // #[error("the data for key `{0}` is not available")]
    // Redaction(String),
    // #[error("invalid header (expected {expected:?}, found {found:?})")]
    // InvalidHeader {
    //     expected: String,
    //     found: String,
    // },
    #[error("Could not find video capture device")]
    DeviceNotFound,
}

// .ok_or_else(|| eyre!("Unable to query webcam for supported resolutions"))?;
// dev.set_format(&fmt).expect("Failed to write format");
// .context("Failed to create buffer stream")?;

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("No supported camera configurations were found")]
    NoSupportedConfiguration,
    #[error("Failed to configure camera settings")]
    SettingsFailure(std::io::Error),
    #[error("Failed to create buffer stream")]
    BufferStreamFailure(std::io::Error),
    #[error("H264 encoder/decoder error")]
    H264EncoderError(#[from] openh264::Error),
    #[error("JPEG decoder error")]
    JPEGDecoderError(#[from] jpeg_decoder::Error),
    #[error("Mmap Stream failed to read")]
    StreamFailure(std::io::Error),
}

pub struct WebcamH264Stream<'a> {
    stream: MmapStream<'a>,
    encoder_mode: EncoderMode,
    pub width: u32,
    pub height: u32,
}

pub enum EncoderMode {
    H264Native(openh264::decoder::Decoder),
    MjpegNative(openh264::encoder::Encoder),
}

pub fn list_devices() -> Vec<PathBuf> {
    let devices = v4l::context::enum_devices();

    devices
        .into_iter()
        .map(|node| node.path().into())
        .collect::<Vec<_>>()
}

pub fn get_device(device_path: &Path) -> Result<Device, DeviceError> {
    let devices = v4l::context::enum_devices();

    let node = devices
        .into_iter()
        .find(|node| node.path() == device_path)
        .ok_or_else(|| DeviceError::DeviceNotFound)?;

    Device::new(node.index()).map_err(|err| DeviceError::CouldNotOpen(err))
}

pub fn stream(mut dev: &mut Device, max_fps: u32) -> Result<WebcamH264Stream, StreamError> {
    let h264 = FourCC::new(b"H264");
    let mjpg = FourCC::new(b"MJPG");

    // Find the largest resolution available for video capture on this device
    let (fourcc, frame_period, width, height) = [h264, mjpg]
        .into_iter()
        // Get an iterator of stepwise or discrete frame sizes
        .filter_map(|fourcc| dev.enum_framesizes(fourcc).ok())
        .flatten()
        // Normalize the frame sizes as discrete
        .flat_map(|framesize| {
            framesize
                .size
                .to_discrete()
                .into_iter()
                .map(move |discrete| (framesize.fourcc.clone(), discrete))
        })
        // Get the frame interval (1 / fps) for each frame size
        .filter_map(|(fourcc, discrete)| {
            (&dev)
                .enum_frameintervals(fourcc, discrete.width, discrete.height)
                .map_err(|err| {
                    warn!(
                        "Unable to get camera frame internals for {}x{}, skipping: {:?}",
                        discrete.width, discrete.height, err
                    )
                })
                .map(move |intervals| intervals.into_iter().map(move |f| (fourcc, f)))
                .ok()
        })
        .flatten()
        .map(|(fourcc, f)| {
            let frame_period = match f.interval {
                FrameIntervalEnum::Discrete(seconds) => seconds,
                FrameIntervalEnum::Stepwise(Stepwise { min, .. }) => min,
            };
            (fourcc, frame_period, f.width, f.height)
        })
        // Filter out frame rates that exceed the max fps
        .filter(|(_, frame_period, _, _)| {
            frame_period.numerator <= max_fps * frame_period.denominator
        })
        // Get the highest resolution & fps within the spec'd fps
        .max_by_key(|(fourcc, frame_period, width, height)| {
            (
                // Prefer larger resolutions
                *width,
                *height,
                // Prefer higher framerates: Scale the fraction so that it doesn't round to zero for sorting purposes
                frame_period.numerator * 1_000_000 / (frame_period.denominator),
                // Prefer H264 over MJPEG
                fourcc == &h264,
            )
        })
        .ok_or_else(|| StreamError::NoSupportedConfiguration)?;

    // Explicitly request the video width, height and fps
    let mut fmt = Format::new(width, height, FourCC::new(b"H264"));

    fmt.fourcc = fourcc;

    dev.set_format(&fmt)
        .map_err(|err| StreamError::SettingsFailure(err))?;

    let params = Parameters::new(frame_period);
    dev.set_params(&params)
        .map_err(|err| StreamError::SettingsFailure(err))?;

    let stream = MmapStream::with_buffers(&mut dev, Type::VideoCapture, 4)
        .map_err(|e| StreamError::BufferStreamFailure(e))?;

    let encoder_mode = if fourcc == h264 {
        let h264_decoder = openh264::decoder::Decoder::new()?;
        EncoderMode::H264Native(h264_decoder)
    } else {
        let h264_encoder_config = openh264::encoder::EncoderConfig::new(width, height);
        let h264_encoder = openh264::encoder::Encoder::with_config(h264_encoder_config)?;
        EncoderMode::MjpegNative(h264_encoder)
    };

    Ok(WebcamH264Stream {
        encoder_mode,
        stream,
        width,
        height,
    })
}

pub enum YUVFrame<'a> {
    Decoded(DecodedYUV<'a>),
    Buffer(YUVBuffer),
}

impl<'a> YUVFrame<'a> {
    /// Encodes the frame as h264 and returns the encoded bitstream.
    pub fn encode_using<'b>(
        &self,
        encoder: &'b mut openh264::encoder::Encoder,
    ) -> Result<EncodedBitStream<'b>, StreamError> {
        match self {
            Self::Decoded(yuv) => Ok(encoder.encode(yuv)?),
            Self::Buffer(yuv) => Ok(encoder.encode(yuv)?),
        }
    }
}

impl<'a> WebcamH264Stream<'a> {
    /// Gets the next H264-encoded bitstream.
    ///
    /// If get_yuv_frame is true then it also returns a YUV image of the latest frame in the returned bitstream.
    ///
    /// The YUV frame is generated from the native bitstream (MJPEG / H264) and should be faster then decoding a frame from the H264
    /// bitstream.
    ///
    /// Even when get_yuv_frame is enabled the yuv frame may still be None if H264 bytes are processed but no frame was produced.
    /// Generally None yuv frame values can be skipped while awaiting a YUV frame.
    pub fn next(
        &mut self,
        get_yuv_frame: bool,
    ) -> Result<(Vec<u8>, Option<YUVFrame>), StreamError> {
        let (buf, _meta) = self
            .stream
            .next()
            .map_err(|e| StreamError::StreamFailure(e))?;

        match &mut self.encoder_mode {
            EncoderMode::H264Native(h264_decoder) => {
                let yuv_frame = if get_yuv_frame {
                    h264_decoder
                        .decode(buf)?
                        .map(|decoded_yuv| YUVFrame::Decoded(decoded_yuv))
                } else {
                    None
                };

                Ok((buf.to_vec(), yuv_frame))
            }
            EncoderMode::MjpegNative(h264_encoder) => {
                let mut jpeg = jpeg_decoder::Decoder::new(buf);
                let rgb_pixels = jpeg.decode()?;

                let yuv_buffer =
                    YUVBuffer::with_rgb(self.width as usize, self.height as usize, &rgb_pixels[..]);

                let h264_bitstream = h264_encoder.encode(&yuv_buffer)?;

                let yuv_frame = if get_yuv_frame {
                    Some(YUVFrame::Buffer(yuv_buffer))
                } else {
                    None
                };

                Ok((h264_bitstream.to_vec(), yuv_frame))
            }
        }
    }
}
