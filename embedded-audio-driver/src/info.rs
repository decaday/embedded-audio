use std::ops::{Div, Mul};

/// Represents metadata information about an audio data stream or file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    /// The sample rate of the audio, in Hz (samples per second).
    pub sample_rate: u32,

    /// The number of channels in the audio (e.g., 1 for mono, 2 for stereo).
    pub channels: u8,

    /// The number of bits per sample (e.g., 8, 16, 24).
    pub bits_per_sample: u8,

    /// The total number of audio frames.
    /// This is `None` if the number of frames is unknown.
    pub num_frames: Option<u32>,
}

impl Default for Info {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            channels: 0,
            bits_per_sample: 0,
            num_frames: None,
        }
    }
}

impl Info {
    pub fn get_alignment_bytes(&self) -> u8 {
        (self.bits_per_sample as u32 * self.channels as u32 / 8) as u8
    }

    pub fn get_bit_rate(&self) -> u32 {
        self.sample_rate as u32 * self.channels as u32 * self.bits_per_sample as u32
    }

    pub fn get_duration_ms(&self) -> Option<u32> {
        self.num_frames.map(|frames| (frames * 1000) / self.sample_rate)
    }

    pub fn down_to_alignment<T>(&self, data: T) -> T 
    where 
        T: Div<Output = T> + Mul<Output = T> + From<u8> + Copy,
    {
        let alignment = T::from(self.get_alignment_bytes());
        data / alignment * alignment
    }
}