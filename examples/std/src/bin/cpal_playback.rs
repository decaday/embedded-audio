use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};

use embedded_audio_driver::element::Element;
use embedded_audio::pipeline::PipelineR2S;
use embedded_audio::stream::CpalOutputStream;
use embedded_audio::stream::cpal_stream::Config;

#[derive(Parser, Debug)]
#[command(version, about = "CPAL feedback example", long_about = None)]
struct Opt {
    /// The output audio device to use
    #[arg(short, long, value_name = "OUT", default_value_t = String::from("default"))]
    output_device: String,

    /// Specify the delay between input and output
    #[arg(short, long, value_name = "DELAY_MS", default_value_t = 50)]
    latency: u32,

    /// Use the JACK host
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    #[arg(short, long)]
    #[allow(dead_code)]
    jack: bool,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    // Conditionally compile with jack if the feature is specified.
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    // Manually check for flags. Can be passed through cargo with -- e.g.
    // cargo run --release --example beep --features jack -- --jack
    let host = if opt.jack {
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    } else {
        cpal::default_host()
    };

    #[cfg(any(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        )),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    let output_device = if opt.output_device == "default" {
        host.default_output_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.output_device).unwrap_or(false))
    }
    .expect("failed to find output device");

    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let mut config: cpal::StreamConfig = output_device.default_output_config().unwrap().into();
    config.channels = 2;
    config.sample_rate = cpal::SampleRate(48000);

    // Build streams.
    println!(
        "Attempting to build both streams with f32 samples and `{:?}`.",
        config
    );
    // Play the streams.
    println!(
        "Starting the input and output streams with `{}` milliseconds of latency.",
        opt.latency
    );
    let cpal_stream = CpalOutputStream::<i16, 2>::new(
        Config {
            rb_capacity: None,
            latency_ms: opt.latency as usize,
        },
        output_device,
        config,
    ).unwrap();

    let info = cpal_stream.get_in_info().unwrap();

    print!("Info: {:#?}", info);

    // let generator = embedded_audio::generator::SineWaveGenerator::new(
    //     info.sample_rate,
    //     info.channels,
    //     info.bits_per_sample,
    //     440.0,  // A4 note
    //     128     // 50% amplitude
    // );

    let wav_data =  include_bytes!("../../../../res/light-rain.wav");
    let mut cursor = embedded_io_adapters::std::FromStd::new(std::io::Cursor::new(wav_data));
    let decoder = embedded_audio::decoder::WavDecoder::new(&mut cursor).expect("Failed to create WavDecoder");
    
    let mut pipeline = PipelineR2S::new(
        decoder,
        cpal_stream,
    );
    pipeline.run().unwrap();
    Ok(())
}