use cpal::traits::{DeviceTrait, HostTrait};
use embassy_executor::Spawner;
use embedded_audio::databus::slot::Slot;
use embedded_audio::decoder::WavDecoder;
use embedded_audio::stream::cpal_output::{Config, CpalOutputStream};
use embedded_audio::transformer::Gain;
use embedded_audio_driver::databus::{Consumer, Producer, Transformer};
use embedded_audio_driver::element::{BaseElement, ProcessStatus::{Eof, Fine}};
use embedded_audio_driver::stream::BaseStream;
use embedded_io_adapters::std::FromStd;
use log::*;

#[embassy_executor::task]
async fn playback_wav() {
    info!("Starting CPAL playback task...");

    // 1. Set up the CPAL host and output device.
    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device available");
    let supported_config = device.default_output_config().expect("no default config");
    let mut config: cpal::StreamConfig = supported_config.into();
    
    config.channels = 2; // stereo
    // config.sample_rate = cpal::SampleRate(44100); // and 44.1kHz sample rate
    config.sample_rate = cpal::SampleRate(48000); // Some devices prefer 48kHz

    info!("Using output device: \"{}\"", device.name().unwrap());
    info!("Using output config: {:?}", config);
    
    // 2. Create the pipeline elements.
    // Source: A WavDecoder reading from an in-memory file.
    let wav_data = include_bytes!("../../../../res/light-rain.wav");
    let cursor = FromStd::new(std::io::Cursor::new(wav_data));
    let mut decoder = WavDecoder::new(cursor);

    // Transformer: A Gain element to increase volume.
    let mut gain = Gain::new(1.3);

    // Sink: A CpalOutputStream to send data to the sound card.
    let mut cpal_stream = CpalOutputStream::<i16, 2>::new(
        Config {
            rb_capacity: None,
            latency_ms: 100,
        },
        device,
        config,
    );

    // 3. Initialize the elements in sequence, passing info downstream.
    decoder.initialize(None).await.expect("Decoder init failed");
    let decoder_info = decoder.get_out_info();
    
    gain.initialize(decoder_info).await.expect("Gain init failed");
    let gain_info = gain.get_out_info();

    cpal_stream.initialize(gain_info).await.expect("CpalStream init failed");
    
    info!("Decoder Info: {:#?}", decoder_info.unwrap());
    info!("Playback starting...");
    
    // 4. Create the databus (a slot with a transformer).
    let mut buffer = vec![0u8; 4096];
    let slot = Slot::new(Some(&mut buffer), true); // `true` enables the transformer stage

    // 5. Set up the ports.
    let mut dec_out_port = slot.out_port();
    let mut gain_inplace_port = slot.inplace_port();
    let mut stream_in_port = slot.in_port();

    // 6. Start the audio stream.
    cpal_stream.start().expect("Failed to start CPAL stream");

    // 7. Run the processing loop.
    loop {
        // Step 1: Decode a chunk of the WAV file into the slot.
        if let Eof = decoder.process(&mut Default::default(), &mut dec_out_port, &mut Default::default()).await.unwrap() {
            // If the decoder is done, we still need to process the last chunk through the rest of the pipeline.
        }

        // Step 2: Apply gain to the data in-place in the slot.
        gain.process(&mut Default::default(), &mut Default::default(), &mut gain_inplace_port).await.unwrap();

        // Step 3: Send the processed chunk from the slot to the CPAL stream.
        match cpal_stream.process(&mut stream_in_port, &mut Default::default(), &mut Default::default()).await.unwrap() {
            Eof => {
                info!("Playback finished.");
                break;
            }
            Fine => { /* Continue processing */ }
        }
    }

    cpal_stream.stop().unwrap();
    info!("Playback loop finished.");
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp_nanos()
        .init();
    spawner.spawn(playback_wav()).unwrap();
}
