use cpal::traits::{DeviceTrait, HostTrait};
use embassy_executor::Spawner;
use log::*;

use embedded_audio::databus::slot::Slot;
use embedded_audio::decoder::WavDecoder;
use embedded_audio::stream::cpal_output::{Config, CpalOutputStream};
use embedded_audio_driver::element::Element;
use embedded_audio_driver::port::{InPlacePort, InPort, OutPort};
use embedded_audio_driver::databus::{Producer, Consumer};
use embedded_audio_driver::stream::Stream;
use embedded_audio_driver::element::ProcessStatus::{Eof, Fine};

// The main task for playing back the WAV file.
#[embassy_executor::task]
async fn playback_wav() {
    info!("Starting CPAL playback task...");

    // 1. Set up the CPAL host and output device.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let supported_config = device.default_output_config().expect("no default config");
    let mut config: cpal::StreamConfig = supported_config.into();
    
    config.channels = 2; // stereo
    // config.sample_rate = cpal::SampleRate(44100); // and 44.1kHz sample rate
    config.sample_rate = cpal::SampleRate(48000); // Some devices prefer 48kHz

    info!("Using output device: \"{}\"", device.name().unwrap());
    info!("Using output config: {:?}", config);
    
    // 2. Create the sink element: a CpalOutputStream.
    // This stream will consume audio data and send it to the sound card.
    let mut cpal_stream = CpalOutputStream::<i16, 2>::new(
        Config {
            rb_capacity: None, // Use default buffer capacity
            latency_ms: 100,
        },
        device,
        config,
    ).expect("Failed to create CPAL output stream");
    
    // The stream must be started to begin playback.
    cpal_stream.start().expect("Failed to start CPAL stream");
    info!("CPAL stream started.");

    // 3. Create the source element: a WavDecoder.
    // Load the WAV file data from an included byte slice.
    let wav_data = include_bytes!("../../../../res/light-rain.wav");
    let mut cursor = embedded_io_adapters::std::FromStd::new(std::io::Cursor::new(wav_data));
    let mut decoder = WavDecoder::new();

    // 4. Create the databus to connect the elements.
    let mut buffer = vec![0u8; 512];
    let slot = Slot::new(Some(&mut buffer), false);
    // Define the input and output ports for this iteration.
    let mut dec_in_port = InPort::new_reader(&mut cursor);
    let mut dec_out_port = slot.out_port();
    
    let mut stream_in_port = slot.in_port();
    let mut stream_out_port = OutPort::new_none();
    let mut inplace_port = InPlacePort::new_none();

    // 5. Run the processing loop.
    // This loop will continue until the decoder reaches the end of the file.
    info!("Starting playback loop...");
    loop {
        // Process the decoder to fill the slot with audio data.
        decoder.process(&mut dec_in_port, &mut dec_out_port, &mut inplace_port).await.unwrap();

        // Process the CPAL stream to consume the data from the slot.
        match cpal_stream.process(&mut stream_in_port, &mut stream_out_port, &mut inplace_port).await.unwrap() {
            Eof => {
                info!("Reached end of WAV file.");
                break;
            }
            Fine => { /* Continue processing */ }
        }
    }
    
    info!("Playback loop finished. The application will now exit.");
    // The stream will stop automatically when CpalOutputStream is dropped.
}

// Embassy's main entry point for the std (desktop) environment.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize a simple logger.
    env_logger::builder()
        .filter_level(log::LevelFilter::Info) // Use Info level to reduce verbosity
        .format_timestamp_nanos()
        .init();

    // Spawn the main playback task.
    spawner.spawn(playback_wav()).unwrap();
}
