extern crate core;

use adder_codec_rs::framer::event_framer::FramerMode::INSTANTANEOUS;
use adder_codec_rs::framer::event_framer::SourceType::U8;
use adder_codec_rs::framer::event_framer::{FrameSequence, Framer, SourceType};
use adder_codec_rs::framer::scale_intensity;
use adder_codec_rs::framer::scale_intensity::FrameValue;
use adder_codec_rs::transcoder::source::framed_source::FramedSource;
use adder_codec_rs::transcoder::source::video::Source;
use adder_codec_rs::SourceCamera::FramedU8;
use adder_codec_rs::{DeltaT, Event, SourceCamera};
use clap::Parser;
use rayon::{current_num_threads, ThreadPool};
use reqwest;
use serde::Serialize;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::{BufWriter, Cursor, Write};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, SendError, Sender};
use std::time::Instant;

/// Command line argument parser
#[derive(Parser, Debug, Default)]
#[clap(author, version, about, long_about = None)]
pub struct MyArgs {
    /// Use color? (For framed input, most likely) (1=yes,0=no)
    #[clap(long, default_value_t = 1)]
    pub(crate) color_input: u32,

    /// Number of ticks per second (should equal ref_time * frame rate)
    #[clap(short, long, default_value_t = 120000)]
    pub(crate) tps: u32,

    #[clap(long, default_value_t = 24)]
    pub(crate) fps: u32,

    /// Number of ticks per input frame // TODO: modularize for different sources
    #[clap(short, long, default_value_t = 5000)]
    pub(crate) ref_time: u32,

    /// Max number of ticks for any event
    #[clap(short, long, default_value_t = 240000)]
    pub(crate) delta_t_max: u32,

    /// Max number of input frames to transcode (0 = no limit)
    #[clap(short, long, default_value_t = 500)]
    frame_count_max: u32,

    /// Index of first input frame to transcode
    #[clap(long, default_value_t = 0)]
    pub(crate) frame_idx_start: u32,

    /// Show live view displays? (1=yes,0=no)
    #[clap(short, long, default_value_t = 0)]
    pub(crate) show_display: u32,

    /// Path to input file
    #[clap(short, long, default_value = "./in.mp4")]
    pub(crate) input_filename: String,

    /// Path to output events file
    #[clap(short, long, default_value = "./out.adder")]
    pub(crate) output_events_filename: String,

    /// Path to output raw video file
    #[clap(short, long, default_value = "./out")]
    pub(crate) output_raw_video_filename: String,

    /// Resize scale
    #[clap(short('z'), long, default_value_t = 0.5)]
    pub(crate) scale: f64,

    /// Positive contrast threshold, in intensity units. How much an intensity must increase
    /// to create a frame division. Only used when look_ahead = 1 and framed input
    #[clap(long, default_value_t = 5)]
    pub(crate) c_thresh_pos: u8,

    /// Negative contrast threshold, in intensity units. How much an intensity must decrease
    /// to create a frame division.  Only used when look_ahead = 1 and framed input
    #[clap(long, default_value_t = 5)]
    pub(crate) c_thresh_neg: u8,
}

async fn download_file() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Download the drop.mp4 video example, if you don't already have it
    let path_str = "./tests/samples/videos/drop.mp4";
    if !Path::new(path_str).exists() {
        let mut resp = reqwest::get("https://www.pexels.com/video/2603664/download/").await?;
        let mut file_out = File::create(path_str).expect("Could not create file on disk");
        let mut data_in = Cursor::new(resp.bytes().await?);
        std::io::copy(&mut data_in, &mut file_out)?;
    }
    Ok(())
}

// Scale down source video for comparison
// ffmpeg -i drop.mp4 -vf scale=960:-1 -crf 0 -c:v libx264 drop_scaled.mp4
// Trim scaled video for comparison (500 frames)
// ffmpeg -i drop_scaled.mp4 -ss 00:00:00 -t 00:00:20.833333 -crf 0 -c:v copy -c:a copy ./drop_scaled_trimmed.mp4

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args: MyArgs = MyArgs::parse();
    println!("c_pos: {}, c_neg: {}", args.c_thresh_pos, args.c_thresh_neg);

    //////////////////////////////////////////////////////
    // Overriding the default args for this particular video example.
    // Can comment out if supplying a local file.
    // download_file().await.unwrap();
    // args.input_filename = "./tests/samples/videos/drop.mp4".to_string();
    // args.output_raw_video_filename = "./tests/samples/videos/drop_out".to_string();
    //////////////////////////////////////////////////////

    let mut source = FramedSource::new(
        args.input_filename,
        None,
        args.frame_idx_start,
        args.ref_time,
        args.tps,
        args.delta_t_max,
        args.scale,
        0,
        args.color_input != 0,
        true,
        args.c_thresh_pos,
        args.c_thresh_neg,
        false,
        true,
        args.show_display != 0,
        SourceCamera::FramedU8,
    )
    .unwrap();

    let width = source.get_video().width;
    let height = source.get_video().height;

    let mut simul_processor = SimulProcessor::new::<u8>(
        source,
        args.ref_time,
        args.tps,
        args.output_raw_video_filename.as_str(),
        args.frame_count_max as i32,
    );

    let now = std::time::Instant::now();
    simul_processor.run().unwrap();

    // Use ffmpeg to encode the raw frame data as an mp4
    let color_str = match args.color_input != 0 {
        true => "bgr24",
        _ => "gray",
    };
    Command::new("sh")
        .arg("-c")
        .arg(
            "ffmpeg -f rawvideo -pix_fmt ".to_owned()
                + &color_str.to_owned()
                + " -s:v "
                + width.to_string().as_str()
                + "x"
                + height.to_string().as_str()
                + " -r 24 -i "
                + &args.output_raw_video_filename
                + " -crf 0 -c:v libx264 -y "
                + &args.output_raw_video_filename
                + ".mp4",
        )
        .spawn()
        .unwrap();
    println!("{} ms elapsed", now.elapsed().as_millis());

    Ok(())
}

pub(crate) struct SimulProcessor {
    source: FramedSource,
    thread_pool: ThreadPool,
    events_tx: Sender<Vec<Vec<Event>>>,
}

impl SimulProcessor {
    pub fn new<T>(
        mut source: FramedSource,
        ref_time: DeltaT,
        tps: DeltaT,
        output_path: &str,
        frame_max: i32,
    ) -> SimulProcessor
    where
        T: Clone + std::marker::Sync + std::marker::Send + 'static,
        T: scale_intensity::FrameValue,
        T: std::default::Default,
        T: std::marker::Copy,
        T: FrameValue<Output = T>,
        T: Serialize,
    {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            // .num_threads(1)
            .num_threads(current_num_threads() / 2)
            .build()
            .unwrap();
        let reconstructed_frame_rate = 24;
        // For instantaneous reconstruction, make sure the frame rate matches the source video rate
        assert_eq!(tps / ref_time, reconstructed_frame_rate);

        let height = source.get_video().height as usize;
        let width = source.get_video().width as usize;
        let channels = source.get_video().channels as usize;

        let mut framer = thread_pool.install(|| {
            FrameSequence::<T>::new(
                height,
                width,
                channels,
                tps,
                reconstructed_frame_rate,
                INSTANTANEOUS,
                U8,
                1,
                FramedU8,
                ref_time,
            )
        });

        let mut output_stream = BufWriter::new(File::create(output_path).unwrap());

        let (events_tx, events_rx): (Sender<Vec<Vec<Event>>>, Receiver<Vec<Vec<Event>>>) =
            channel();
        let mut now = Instant::now();

        // Spin off a thread for managing the input frame buffer. It will keep the buffer filled,
        // and pre-process the next input frame (grayscale conversion and rescaling)
        rayon::spawn(move || {
            let mut frame_count = 1;
            loop {
                match events_rx.recv() {
                    Ok(events) => {
                        // assert_eq!(events.len(), (self.source.get_video().height as f64 / self.framer.chunk_rows as f64).ceil() as usize);

                        // Frame the events
                        if framer.ingest_events_events(events) {
                            match framer.write_multi_frame_bytes(&mut output_stream) {
                                0 => {
                                    panic!("Should have frame, but didn't")
                                }
                                frames_returned => {
                                    frame_count += frames_returned;
                                    print!(
                                        "\rOutput frame {}. Got {} frames in  {}ms",
                                        frame_count,
                                        frames_returned,
                                        now.elapsed().as_millis()
                                    );
                                    io::stdout().flush().unwrap();
                                    now = Instant::now();
                                }
                            }
                        }
                        if frame_count >= frame_max && frame_max > 0 {
                            eprintln!("Wrote max frames. Exiting channel.");
                            break;
                        }
                    }
                    Err(_) => {
                        eprintln!("Event receiver is closed. Exiting channel.");
                        break;
                    }
                };
            }
        });

        SimulProcessor {
            source,
            thread_pool,
            events_tx,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut now = Instant::now();

        loop {
            match self.thread_pool.install(|| self.source.consume(1)) {
                Ok(events) => {
                    // self.framify_new_events(events, output_1.0)
                    match self.events_tx.send(events) {
                        Ok(_) => {}
                        Err(_) => {
                            break;
                        }
                    };
                }
                Err("End of video") => break, // TODO: make it a proper rust error
                Err(_) => {}
            };

            let video = self.source.get_video();

            if video.in_interval_count % 30 == 0 {
                print!(
                    "\rFrame {} in  {}ms",
                    video.in_interval_count,
                    now.elapsed().as_millis()
                );
                io::stdout().flush().unwrap();
                now = Instant::now();
            }
        }

        println!("Closing stream...");
        self.source.get_video_mut().end_write_stream();
        println!("FINISHED");

        Ok(())
    }
}
