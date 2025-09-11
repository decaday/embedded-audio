use core::ops::{Div, Mul, Rem};

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
    pub num_frames: Option<u64>,
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
    /// Creates a new `Info` instance with the specified parameters.
    /// 
    /// # Parameters
    /// - `sample_rate`: The sample rate in Hz. 1 for mono, 2 for stereo.
    /// - `channels`: The number of audio channels.
    /// - `bits_per_sample`: The number of bits per sample.
    /// - `num_frames`: The total number of audio frames, or `None` if unknown.
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8, num_frames: Option<u64>) -> Self {
        Self {
            sample_rate,
            channels,
            bits_per_sample,
            num_frames,
        }
    }

    pub fn set_duration_ms(&mut self, duration_ms: u32) {
        self.num_frames = Some(((duration_ms * self.sample_rate) / 1000) as _);
    }

    pub fn set_num_frames(&mut self, num_frames: u64) {
        self.num_frames = Some(num_frames);
    }

    pub fn set_duration_s(&mut self, duration_s: f32) {
        self.num_frames = Some((duration_s * self.sample_rate as f32) as _);
    }

    pub fn vaild(&self) -> bool {
        self.sample_rate > 0 && self.channels > 0 && self.bits_per_sample > 0 && (self.bits_per_sample % 8 == 0)
    }

    pub fn get_alignment_bytes(&self) -> u8 {
        (self.bits_per_sample as u32 * self.channels as u32 / 8) as u8
    }

    pub fn get_bit_rate(&self) -> u32 {
        self.sample_rate as u32 * self.channels as u32 * self.bits_per_sample as u32
    }

    pub fn get_duration_ms(&self) -> Option<u32> {
        self.num_frames.map(|frames| ((frames * 1000) / self.sample_rate as u64) as _)
    }

    pub fn down_to_alignment<T>(&self, data: T) -> T 
    where 
        T: Div<Output = T> + Mul<Output = T> + From<u8> + Copy,
    {
        let alignment = T::from(self.get_alignment_bytes());
        data / alignment * alignment
    }

    pub fn is_aligned<T>(&self, data: T) -> bool 
    where 
        T: Rem<Output = T> + From<u8> + Copy + PartialEq,
    {
        let alignment = T::from(self.get_alignment_bytes());
        data % alignment == T::from(0)
    }
}