use anyhow::Result;
use slog::{debug, info, o, Drain, Logger};
use structopt::StructOpt;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

use tsclientlib::ClientId;
use tsproto_packets::packets::{Direction, InAudioBuf};

mod audio_utils;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ConnectionId(u64);

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The address of the server to connect to
	#[structopt(short = "a", long, default_value = "localhost")]
	address: String,
	/// The volume for the capturing
	#[structopt(default_value = "1.0")]
	volume: f32,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[structopt(short = "v", long, parse(from_occurrences))]
	verbose: u8,
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_envlogger::new(drain).fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		Logger::root(drain, o!())
	};

	let con_id = ConnectionId(0);
	let local_set = LocalSet::new();
	let audiodata = audio_utils::start(logger.clone(), &local_set)?;

	let (send, mut recv) = mpsc::channel(1);
	{
		let mut a2t = audiodata.a2ts.lock().unwrap();
		a2t.set_listener(send);
		a2t.set_volume(args.volume);
		a2t.set_playing(true);
	}

	let t2a = audiodata.ts2a.clone();
	loop {
		// Wait for ctrl + c
		tokio::select! {
			send_audio = recv.recv() => {
				if let Some(packet) = send_audio {
					let from = ClientId(0);
					let mut t2a = t2a.lock().unwrap();
					let in_audio = InAudioBuf::try_new(Direction::C2S, packet.into_vec()).unwrap();
					if let Err(e) = t2a.play_packet((con_id, from), in_audio) {
						debug!(logger, "Failed to play packet"; "error" => %e);
					}
				} else {
					info!(logger, "Audio sending stream was canceled");
					break;
				}
			}
			_ = tokio::signal::ctrl_c() => { break; }
		};
	}

	Ok(())
}
