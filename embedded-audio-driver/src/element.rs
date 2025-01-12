// use ringbuffer::AllocRingBuffer as RingBuffer;

/// Represents metadata information about an audio data stream or file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    /// The sample rate of the audio, in Hz (samples per second).
    pub sample_rate: u32,

    /// The number of channels in the audio (e.g., 1 for mono, 2 for stereo).
    pub channels: u8,

    /// The number of bits per sample (e.g., 8, 16, 24).
    pub bits_per_sample: u8,

    /// The total duration of the audio, in milliseconds.
    /// This is `None` if the duration is unknown (e.g., for streaming audio).
    pub duration: Option<u32>,

    /// The total number of audio frames.
    /// This is `None` if the number of frames is unknown.
    pub num_frames: Option<u32>,

    // /// The bitrate of the audio, in kbps (kilobits per second).
    // /// This is `None` if the bitrate is not applicable or unknown.
    // pub bitrate: Option<u32>,
}

impl Default for Info {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            channels: 0,
            bits_per_sample: 0,
            duration: None,
            num_frames: None,
            // bitrate: None,
        }
    }
}

pub trait Element {
    fn get_in_info(&self) -> Option<Info>;

    fn get_out_info(&self) -> Option<Info>;

    // fn progress(&mut self, in_ringbuffer: &mut RingBuffer<u8>, out_ringbuffer: &mut RingBuffer<u8>);
}