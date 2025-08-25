use std::fs::File;

use embassy_executor::Spawner;
use embedded_io_adapters::std::FromStd;
use log::*;

use embedded_audio::databus::slot::Slot;
use embedded_audio::encoder::WavEncoder;
use embedded_audio::generator::SineWaveGenerator;
use embedded_audio_driver::element::Element;
use embedded_audio_driver::element::ProcessStatus::{Eof, Fine};
use embedded_audio_driver::port::{Dmy, InPort, OutPort};

// The main task for generating the WAV file.
// It sets up the pipeline and runs the processing loop.
#[embassy_executor::task]
async fn generate_wav() {
    info!("Starting WAV generation task...");

    // 1. Create the source element: a sine wave generator with parameters defined inline.
    let mut generator = SineWaveGenerator::new(
        44100, // sample_rate
        2,     // channels
        16,    // bits_per_sample
        440.0, // frequency (A4 note)
        0.5,   // amplitude (50%)
    );
    generator.set_total_secs(Some(2f32)); // Generate 2 seconds of audio

    // Retrieve audio format info from the generator.
    let info = generator.get_out_info().expect("Generator should provide output info");
    info!("Generator Info: {:#?}", info);
    info!("Alignment (bytes per frame): {}", info.get_alignment_bytes());
    info!("Bit rate: {}", info.get_bit_rate());

    // 2. Create the sink element: a WAV file encoder.
    // Ensure the output directory exists.
    let path = std::path::Path::new("temp");
    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }

    // Create the output file and wrap it for embedded_io compatibility.
    let file = File::create("temp/sine_wave_A4.wav").expect("Failed to create file");
    let mut file_writer = FromStd::new(file);
    
    let mut encoder = WavEncoder::new();
    encoder.set_info(info).expect("Failed to set info for encoder");

    // 3. Create the databus to connect the elements.
    // The buffer size determines how much data is processed in each step.
    let mut buffer = vec![0u8; 4096];
    let slot = Slot::new(Some(&mut buffer));
    let mut gen_in_port: InPort<Dmy, Dmy> = InPort::None;
    let mut gen_out_port:OutPort<Dmy, _>  = OutPort::Payload(&slot);
    
    let mut enc_in_port: InPort<Dmy, _> = InPort::Payload(&slot);
    let mut enc_out_port: OutPort<_, Dmy> = OutPort::Writer(&mut file_writer);

    loop {
        // Define the input and output ports for the elements for this iteration.

        // Process the generator to fill the slot with audio data.
        generator.process(&mut gen_in_port, &mut gen_out_port).await.unwrap();

        // Process the encoder to write the data from the slot to the file.

        match encoder.process(&mut enc_in_port, &mut enc_out_port).await.unwrap() {
            Eof => {
                info!("Reached end of audio generation.");
                break;
            }
            Fine => { /* Continue processing */ }
        }
    }
    
    // 5. Finalize the WAV file.
    // This updates the header with the correct file and data chunk sizes.
    encoder.finalize(&mut file_writer).expect("Failed to finalize WAV header");

    info!("Finished generating temp/sine_wave_A4.wav");
}

// Embassy's main entry point for the std (desktop) environment.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize a simple logger for debug output.
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .format_timestamp_nanos()
        .init();

    // Spawn the main task.
    spawner.spawn(generate_wav()).unwrap();
}
