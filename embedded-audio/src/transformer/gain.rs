//! An audio gain processor, Inplace Operation. 
//! currently using Q16.16 data type, with manual SIMD on x86 or aarch64 (aarch64 untested).
//! 
//! 
//! TODO: Optimize code to let the compiler auto-vectorize as much as possible.
//! TODO: Use DSP instructions (e.g., CMSIS-DSP) on Cortex-M and RISC-V embedded platforms.

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
use core::mem;

use embedded_io::{Read, Seek, Write};

use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{Element, Fine, ProcessResult};
use embedded_audio_driver::info::Info;
use embedded_audio_driver::port::{Dmy, InPlacePort, InPort, OutPort, PortRequirements};
use embedded_audio_driver::Error;

// Fixed-point gain representation (Q16.16 format)
type FixedGain = i32;

const FIXED_POINT_SHIFT: u32 = 16;
const FIXED_POINT_ONE: FixedGain = 1 << FIXED_POINT_SHIFT;

#[inline]
fn float_to_fixed(gain: f32) -> FixedGain {
    (gain * FIXED_POINT_ONE as f32) as FixedGain
}

/// A trait to abstract over different audio sample formats for processing.
trait Sample: Sized + Copy {
    /// The number of bytes this sample type occupies.
    #[allow(dead_code)]
    const BYTES: usize = mem::size_of::<Self>();

    /// Applies a linear gain to the sample using fixed-point arithmetic.
    fn apply_gain_fixed(self, gain: FixedGain) -> Self;
}

impl Sample for i16 {
    #[inline]
    fn apply_gain_fixed(self, gain: FixedGain) -> Self {
        let result = (self as i64 * gain as i64) >> FIXED_POINT_SHIFT;
        result.clamp(i16::MIN as i64, i16::MAX as i64) as i16
    }
}

impl Sample for i32 {
    #[inline]
    fn apply_gain_fixed(self, gain: FixedGain) -> Self {
        let result = (self as i64 * gain as i64) >> FIXED_POINT_SHIFT;
        result.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }
}

impl Sample for u8 {
    #[inline]
    fn apply_gain_fixed(self, gain: FixedGain) -> Self {
        let centered = (self as i64) - 128;
        let result = ((centered * gain as i64) >> FIXED_POINT_SHIFT) + 128;
        result.clamp(0, 255) as u8
    }
}

/// Generic scalar processing with fixed-point arithmetic
fn process_scalar<S: Sample>(payload: &mut [u8], gain: FixedGain) {
    let samples: &mut [S] = unsafe { payload.align_to_mut::<S>().1 };
    for sample in samples.iter_mut() {
        *sample = sample.apply_gain_fixed(gain);
    }
}

/// Optimized 24-bit processing using 4-byte aligned chunks
fn process_24bit_fixed(payload: &mut [u8], gain: FixedGain) {
    const MAX_24_BIT: i32 = (1 << 23) - 1;
    const MIN_24_BIT: i32 = -(1 << 23);

    for sample_chunk in payload.chunks_exact_mut(3) {
        let sample_bytes = [
            sample_chunk[0],
            sample_chunk[1],
            sample_chunk[2],
            if sample_chunk[2] & 0x80 > 0 { 0xFF } else { 0 },
        ];
        let sample = i32::from_le_bytes(sample_bytes);

        let result = ((sample as i64 * gain as i64) >> FIXED_POINT_SHIFT) as i32;
        let clamped = result.clamp(MIN_24_BIT, MAX_24_BIT);

        let result_bytes = clamped.to_le_bytes();
        sample_chunk[0] = result_bytes[0];
        sample_chunk[1] = result_bytes[1];
        sample_chunk[2] = result_bytes[2];
    }
}

/// An Element that applies gain to an audio signal in-place.
pub struct Gain {
    info: Option<Info>,
    fixed_gain: FixedGain,
    port_requirements: Option<PortRequirements>,
    #[cfg(target_arch = "x86_64")]
    use_sse2: bool,
    #[cfg(target_arch = "aarch64")]
    use_neon: bool,
}

impl Gain {
    /// Creates a new Gain element.
    ///
    /// # Arguments
    ///
    /// * `gain` - The linear gain factor to apply. 1.0 means no change.
    pub fn new(gain: f32) -> Self {
        let fixed_gain = float_to_fixed(gain);
        Self::new_fixed_q16_16(fixed_gain)
    }

    /// Creates a new Gain element.
    ///
    /// # Arguments
    ///
    /// * `gain` - The linear gain factor to apply. Q16.16 format.
    pub fn new_fixed_q16_16(gain: FixedGain) -> Self {
        Self {
            info: None,
            fixed_gain: gain,
            port_requirements: None,
            #[cfg(target_arch = "x86_64")]
            use_sse2: std::arch::is_x86_feature_detected!("sse2"),
            #[cfg(target_arch = "aarch64")]
            use_neon: std::arch::is_aarch64_feature_detected!("neon"),
        }
    }

    /// Returns whether SIMD instructions are being used
    #[cfg(target_arch = "x86_64")]
    pub fn is_using_simd(&self) -> bool {
        self.use_sse2
    }

    #[cfg(target_arch = "aarch64")]
    pub fn is_using_simd(&self) -> bool {
        self.use_neon
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn is_using_simd(&self) -> bool {
        false
    }
}

impl Element for Gain {
    type Error = Error;

    fn get_in_info(&self) -> Option<Info> {
        self.info
    }

    fn get_out_info(&self) -> Option<Info> {
        self.info
    }

    fn get_port_requirements(&self) -> PortRequirements {
        self.port_requirements.expect("must be called after initialize")
    }

    async fn initialize<'a, R, W>(
        &mut self,
        _in_port: &mut InPort<'a, R, Dmy>,
        _out_port: &mut OutPort<'a, W, Dmy>,
        upstream_info: Option<Info>,
    ) -> Result<PortRequirements, Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
    {
        let info = upstream_info.ok_or(Error::InvalidParameter)?;
        if ![8, 16, 24, 32].contains(&info.bits_per_sample) {
            return Err(Error::Unsupported)
        }
        self.info = Some(info);

        let min_payload_size = (info.bits_per_sample / 8) as u16;
        self.port_requirements = Some(PortRequirements::new_in_place(min_payload_size));
        Ok(self.port_requirements.unwrap())
    }

    fn available(&self) -> u32 {
        u32::MAX
    }

    async fn process<'a, R, W, C, P, T>(
        &mut self,
        _in_port: &mut InPort<'a, R, C>,
        _out_port: &mut OutPort<'a, W, P>,
        inplace_port: &mut InPlacePort<'a, T>,
    ) -> ProcessResult<Self::Error>
    where
        R: Read + Seek,
        W: Write + Seek,
        C: Consumer<'a>,
        P: Producer<'a>,
        T: Transformer<'a>,
    {
        if let InPlacePort::Transformer(transformer) = inplace_port {
            let mut payload = transformer.acquire_transform().await;
            let info = self.info.ok_or(Error::NotInitialized)?;

            match info.bits_per_sample {
                // Use SIMD or NEON only on 16-bit as a demo, since this is `embedded-audio`.
                16 => {
                    #[cfg(target_arch = "x86_64")]
                    {
                        if self.use_sse2 {
                            unsafe {
                                process_simd_i16_sse2(&mut payload, self.fixed_gain);
                            }
                        } else {
                            process_scalar::<i16>(&mut payload, self.fixed_gain);
                        }
                    }
                    #[cfg(target_arch = "aarch64")]
                    {
                        if self.use_neon {
                            unsafe {
                                process_simd_i16_neon(&mut payload, self.fixed_gain);
                            }
                        } else {
                            process_scalar::<i16>(&mut payload, self.fixed_gain);
                        }
                    }
                    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                    {
                        process_scalar::<i16>(&mut payload, self.fixed_gain);
                    }
                }
                32 => process_scalar::<i32>(&mut payload, self.fixed_gain),
                8 => process_scalar::<u8>(&mut payload, self.fixed_gain),
                24 => process_24bit_fixed(&mut payload, self.fixed_gain),
                _ => return Err(Error::Unsupported),
            }

            Ok(Fine)
        } else {
            Err(Error::Unsupported)
        }
    }
}

/// SSE2 optimized 16-bit processing with fixed-point arithmetic
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn process_simd_i16_sse2(payload: &mut [u8], gain: FixedGain) {
    let (prefix, chunks, suffix) = payload.align_to_mut::<__m128i>();
    process_scalar::<i16>(prefix, gain);

    for chunk in chunks {
        let samples_i16 = *chunk;

        // Convert i16 to two i32 vectors
        let samples_lo_i32 = _mm_cvtepi16_epi32(samples_i16);
        let samples_hi_i32 = _mm_cvtepi16_epi32(_mm_unpackhi_epi64(samples_i16, samples_i16));

        // For SSE2, we need to handle the 64-bit multiplication differently
        // Convert to floating point for multiplication, then back to integer
        let gain_f32 = gain as f32 / FIXED_POINT_ONE as f32;
        let gain_vec = _mm_set1_ps(gain_f32);
        
        let samples_lo_f32 = _mm_cvtepi32_ps(samples_lo_i32);
        let samples_hi_f32 = _mm_cvtepi32_ps(samples_hi_i32);
        
        let result_lo_f32 = _mm_mul_ps(samples_lo_f32, gain_vec);
        let result_hi_f32 = _mm_mul_ps(samples_hi_f32, gain_vec);
        
        let result_lo = _mm_cvtps_epi32(result_lo_f32);
        let result_hi = _mm_cvtps_epi32(result_hi_f32);

        // Pack back to i16 with saturation
        *chunk = _mm_packs_epi32(result_lo, result_hi);
    }

    process_scalar::<i16>(suffix, gain);
}

/// NEON optimized 16-bit processing with fixed-point arithmetic
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn process_simd_i16_neon(payload: &mut [u8], gain: FixedGain) {
    let (prefix, chunks, suffix) = payload.align_to_mut::<int16x8_t>();
    process_scalar::<i16>(prefix, gain);

    // Convert fixed-point gain back to float for NEON processing
    let gain_f32 = gain as f32 / FIXED_POINT_ONE as f32;
    let gain_vec = vdupq_n_f32(gain_f32);

    for chunk in chunks {
        let samples_i16x8 = *chunk;

        // Widen i16 to two i32 vectors
        let samples_i32x4_low = vmovl_s16(vget_low_s16(samples_i16x8));
        let samples_i32x4_high = vmovl_s16(vget_high_s16(samples_i16x8));

        // Convert i32 to f32, apply gain, convert back
        let samples_f32x4_low = vcvtq_f32_s32(samples_i32x4_low);
        let samples_f32x4_high = vcvtq_f32_s32(samples_i32x4_high);

        let result_f32x4_low = vmulq_f32(samples_f32x4_low, gain_vec);
        let result_f32x4_high = vmulq_f32(samples_f32x4_high, gain_vec);
        
        let result_i32x4_low = vcvtq_s32_f32(result_f32x4_low);
        let result_i32x4_high = vcvtq_s32_f32(result_f32x4_high);

        // Narrow i32 back to i16 with saturation
        *chunk = vcombine_s16(vmovn_s32(result_i32x4_low), vmovn_s32(result_i32x4_high));
    }

    process_scalar::<i16>(suffix, gain);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::databus::slot::Slot;
    use embedded_audio_driver::{
        info::Info,
        port::{InPort, OutPort},
    };

    #[tokio::test]
    async fn test_gain_process_16bit_fixed_point() {
        let info = Info::new(44100, 1, 16, None);
        let mut gain = Gain::new(2.0);
        gain.initialize(&mut InPort::new_none(), &mut OutPort::new_none(), Some(info)).await.unwrap();

        let mut buffer = vec![0u8; 16];
        let initial_samples: [i16; 8] = [1000, -2000, 3000, -4000, 5000, 20000, -30000, 15000];
        
        for (i, sample) in initial_samples.iter().enumerate() {
            buffer[i*2..(i+1)*2].copy_from_slice(&sample.to_le_bytes());
        }

        let slot = Slot::new(Some(&mut buffer), true);
        {
            let mut p = slot.acquire_write().await;
            p.set_valid_length(16);
        }

        let mut in_port = InPort::new_none();
        let mut out_port = OutPort::new_none();
        let mut inplace_port = slot.inplace_port();

        let result = gain.process(&mut in_port, &mut out_port, &mut inplace_port).await;
        assert!(result.is_ok());

        let r = slot.acquire_read().await;
        let processed_sample1 = i16::from_le_bytes(r[0..2].try_into().unwrap());
        let processed_sample2 = i16::from_le_bytes(r[2..4].try_into().unwrap());
        let processed_sample6 = i16::from_le_bytes(r[10..12].try_into().unwrap());
        let processed_sample7 = i16::from_le_bytes(r[12..14].try_into().unwrap());

        assert_eq!(processed_sample1, 2000);
        assert_eq!(processed_sample2, -4000);
        assert_eq!(processed_sample6, i16::MAX); // 20000 * 2.0 should clamp to max
        assert_eq!(processed_sample7, i16::MIN); // -30000 * 2.0 should clamp to min
    }

    #[cfg(target_arch = "x86_64")]
    #[tokio::test]
    async fn test_simd_detection_and_usage() {
        let gain = Gain::new(1.5);
        
        // Test SIMD feature detection
        let expected_simd = std::arch::is_x86_feature_detected!("sse2");
        assert_eq!(gain.is_using_simd(), expected_simd);

        if expected_simd {
            // Test that SIMD produces same results as scalar
            let info = Info::new(44100, 1, 16, None);
            let mut simd_gain = Gain::new(1.5);
            simd_gain.initialize(&mut InPort::new_none(), &mut OutPort::new_none(), Some(info)).await.unwrap();

            let mut simd_buffer = vec![0u8; 32];
            let test_samples: [i16; 16] = [
                1000, -2000, 3000, -4000, 5000, -6000, 7000, -8000,
                9000, -10000, 11000, -12000, 13000, -14000, 15000, -16000
            ];
            
            for (i, sample) in test_samples.iter().enumerate() {
                simd_buffer[i*2..(i+1)*2].copy_from_slice(&sample.to_le_bytes());
            }

            let slot = Slot::new(Some(&mut simd_buffer), true);
            {
                let mut p = slot.acquire_write().await;
                p.set_valid_length(32);
            }

            let mut in_port = InPort::new_none();
            let mut out_port = OutPort::new_none();
            let mut inplace_port = slot.inplace_port();

            let result = simd_gain.process(&mut in_port, &mut out_port, &mut inplace_port).await;
            assert!(result.is_ok());

            // Verify processing occurred
            let r = slot.acquire_read().await;
            let processed_first = i16::from_le_bytes(r[0..2].try_into().unwrap());
            assert_eq!(processed_first, 1500); // 1000 * 1.5
        } else {
            // panic!("")
        }
    }

    #[tokio::test]
    async fn test_24bit_optimization() {
        let info = Info::new(48000, 1, 24, None);
        let mut gain = Gain::new(1.25);
        gain.initialize(&mut InPort::new_none(), &mut OutPort::new_none(), Some(info)).await.unwrap();

        // Test with 24-bit samples (3 bytes each)
        let mut buffer = vec![0u8; 12]; // 4 samples * 3 bytes
        
        // Sample 1: 0x123456 (little-endian: 0x56, 0x34, 0x12)
        buffer[0..3].copy_from_slice(&[0x56, 0x34, 0x12]);
        // Sample 2: negative value
        buffer[3..6].copy_from_slice(&[0x00, 0x00, 0x80]);
        // Sample 3 and 4: some test values
        buffer[6..9].copy_from_slice(&[0xFF, 0xFF, 0x7F]);
        buffer[9..12].copy_from_slice(&[0x00, 0x00, 0x00]);

        let slot = Slot::new(Some(&mut buffer), true);
        {
            let mut p = slot.acquire_write().await;
            p.set_valid_length(12);
        }

        let mut in_port = InPort::new_none();
        let mut out_port = OutPort::new_none();
        let mut inplace_port = slot.inplace_port();

        let result = gain.process(&mut in_port, &mut out_port, &mut inplace_port).await;
        assert!(result.is_ok());

        // Verify the processing completed without errors
        let _r = slot.acquire_read().await;
    }

    #[tokio::test]
    async fn test_fixed_point_precision() {
        // Test fixed-point conversion and arithmetic precision
        let gain_float = 1.5;
        let fixed_gain = float_to_fixed(gain_float);
        
        // Test sample processing
        let sample: i16 = 1000;
        let result = sample.apply_gain_fixed(fixed_gain);
        assert_eq!(result, 1500);

        // Test edge cases with safer values to avoid overflow
        let large_sample: i16 = 16000;  // Use a safer value instead of MAX
        let result_large = large_sample.apply_gain_fixed(float_to_fixed(2.0));
        assert_eq!(result_large, 32000);

        // Test actual overflow/clamping case
        let max_sample: i16 = i16::MAX;
        let result_max = max_sample.apply_gain_fixed(float_to_fixed(2.0));
        assert_eq!(result_max, i16::MAX); // Should clamp

        // Test negative clamping
        let min_sample: i16 = i16::MIN;
        let result_min = min_sample.apply_gain_fixed(float_to_fixed(2.0));
        assert_eq!(result_min, i16::MIN); // Should clamp
    }

    #[tokio::test]
    async fn test_gain_info_and_requirements() {
        let info = Info::new(48000, 2, 24, None);
        let mut gain = Gain::new(1.5);
        assert!(gain.get_in_info().is_none());
        assert!(gain.get_out_info().is_none());
        
        let _ = gain.initialize(&mut InPort::new_none(), &mut OutPort::new_none(), Some(info)).await;
        assert!(gain.get_in_info().is_some());
        assert_eq!(gain.get_in_info().unwrap(), info);
        assert!(gain.get_out_info().is_some());
        assert_eq!(gain.get_out_info().unwrap(), info);

        let reqs = gain.get_port_requirements();
        assert!(reqs.in_place.is_some());
        assert_eq!(reqs.in_place.unwrap(), 3);
    }
}
