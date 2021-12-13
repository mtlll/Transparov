use log::{info, SetLoggerError};
use simplelog::{Config, LevelFilter, WriteLogger};
use std::fs;

use fibers::io::stdin;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::{io, panic};
use vampirc_uci::{parse_one, UciMessage};

use engine::Engine;

mod engine;

enum EngineError {
    IOError(io::Error),
    LoggerError(SetLoggerError),
}
impl From<io::Error> for EngineError {
    fn from(err: io::Error) -> Self {
        EngineError::IOError(err)
    }
}

impl From<SetLoggerError> for EngineError {
    fn from(err: SetLoggerError) -> Self {
        EngineError::LoggerError(err)
    }
}

fn main() {
    init_logger();
    uci_loop();
}
fn init_logger() -> Result<(), EngineError> {
    let path = Path::new("engine.log");

    /* backup the previous log */
    if path.exists() {
        fs::rename(path, "engine.log.old")?;
    }

    let logfile = fs::File::create(path)?;
    let _ = WriteLogger::init(LevelFilter::Info, Config::default(), logfile)?;
    panic::set_hook(Box::new(|panic_info| {
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            if let Some(loc) = panic_info.location() {
                info!(
                    "panic occurred in thread: {:?} in {} at line {}: {:?}",
                    thread::current().id(),
                    loc.file(),
                    loc.line(),
                    s
                );
            } else {
                info!("panic occurred: {:?}", s);
            }
        } else {
            info!("panic occurred");
        }
    }));
    Ok(())
}

fn uci_loop() -> io::Result<()> {
    let mut input = String::new();
    let running = Arc::new(AtomicBool::new(true));
    let (handle, tx) = {
        let engine = Engine::default();
        engine.start()
    };
    let mut stdin = BufReader::new(stdin());

    {
        let tx = tx.clone();
        let running = running.clone();

        ctrlc::set_handler(move || {
            info!("received SIGINT/SIGTERM. Quitting...");
            tx.send(UciMessage::Quit);
            running.store(false, Ordering::Relaxed);
            info!("Told main thread to quit.");
        });
    }

    while running.load(Ordering::Acquire) {
        if stdin.read_line(&mut input).is_err() {
            thread::sleep(Duration::from_millis(2));
        } else {
            if input.starts_with("quit") {
                running.store(false, Ordering::Release);
            }
            let message = parse_one(&input);

            if tx.send(message).is_err() {
                break;
            }

            input.clear();
        }
    }

    info!("Joining engine controller thread...");
    handle.join();
    Ok(())
}
