use embedded_audio_driver::element::ReaderElement;
use embedded_audio_driver::encoder::Encoder;
use embedded_audio::encoder::WavEncoder;
use embedded_audio::generator::sine_wave::SineWaveGenerator;
use embedded_audio::relay::Relay;

use embedded_io_adapters::std::FromStd;

fn main() {
    let generator = SineWaveGenerator::new(
        44100,  // CD quality sample rate
        2,      // Stereo
        16,     // 16-bit audio
        440.0,  // A4 note
        128     // 50% amplitude
    );

    let info = generator.get_info();
    println!("Info: {:#?}", info);
    println!("Info get_alignment_bytes: {:#?} ", info.get_alignment_bytes());
    println!("Info get_bit_rate: {:#?}", info.get_bit_rate());
    
    let path = std::path::Path::new("temp");
    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }
    
    let file = std::fs::File::create("temp/sine_wave_A4.wav").unwrap();
    let mut file_adapter = FromStd::new(file);
    let mut encoder = WavEncoder::new(&mut file_adapter, info).unwrap();

    let mut relay = Relay::<1024>::new(generator, encoder_reader).unwrap();

    // let mut pipeline = PipelineR2AR2W::new(generator, relay,  encoder_reader);
    // pipeline.run();

    relay.process().unwrap();
    encoder.stop().unwrap();
    
    println!("Finished");
}