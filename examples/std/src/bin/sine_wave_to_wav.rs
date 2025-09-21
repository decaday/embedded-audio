use std::fs::File;

use embassy_executor::Spawner;
use embedded_audio::databus::slot::Slot;
use embedded_audio::encoder::WavEncoder;
use embedded_audio::generator::SineWaveGenerator;
use embedded_audio_driver::databus::{Producer, Consumer};
use embedded_audio_driver::element::{BaseElement, Eof};
use embedded_audio_driver::info::Info;
use embedded_io_adapters::std::FromStd;
use log::*;

#[embassy_executor::task]
async fn generate_wav() {
    info!("Starting WAV generation task...");

    // 1. Create the source element: a sine wave generator with parameters defined inline.
    let mut info = Info::new(
        44100, // sample_rate
        2, // channels
        16, // bits_per_sample
        None // num_frames, we set it later by duration
    );
    info.set_duration_s(5.0); // 5 seconds duration
    
    info!("Generator Info: {:#?}", info);
    info!("Alignment (bytes per frame): {}", info.get_alignment_bytes());
    info!("Bit rate: {}", info.get_bit_rate());
    
    let mut generator = SineWaveGenerator::new(
        info,
        440.0, // frequency (A4 note)
        0.5,   // amplitude
    );

    // 2. Create a WAV file encoder.
    let path = std::path::Path::new("temp");
    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }
    let file = File::create("temp/sine_wave_A4.wav").expect("Failed to create file");
    let file_writer = FromStd::new(file);
    let mut encoder = WavEncoder::new(file_writer);

    // 3. Initialize the elements in sequence.
    // The generator is a source, so it has no upstream info.
    generator.initialize(None).await.expect("Generator failed to initialize");
    
    // The encoder's input format is determined by the generator's output format.
    let generator_info = generator.get_out_info();
    encoder.initialize(generator_info).await.expect("Encoder failed to initialize");
    
    info!("Generator Info: {:#?}", generator_info.unwrap());

    // 4. Create the databus to connect the elements.
    let mut buffer = vec![0u8; 4096];
    let slot = Slot::new(Some(&mut buffer), false);

    // 5. Set up the ports for the processing loop.
    let mut gen_out_port = slot.out_port();
    let mut enc_in_port = slot.in_port();
    
    info!("Starting processing loop...");
    loop {
        // Step 1: Process the generator to fill the slot with audio data.
        let gen_status = generator.process(&mut Default::default(), &mut gen_out_port, &mut Default::default()).await.unwrap();

        // Step 2: Process the encoder to write the data from the slot to the file.
        let enc_status = encoder.process(&mut enc_in_port, &mut Default::default(), &mut Default::default()).await.unwrap();

        // If both elements report Eof, the pipeline is complete.
        if gen_status == Eof || enc_status == Eof {
            info!("Reached end of audio generation.");
            break;
        }
    }

    // 6. Finalize the WAV file (optional, but good practice).
    // This updates the header. Eof handling in `process` should also do this.
    encoder.finalize().expect("Failed to finalize WAV header");

    info!("Finished generating temp/sine_wave_A4.wav");
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_nanos()
        .init();
    spawner.spawn(generate_wav()).unwrap();
}
