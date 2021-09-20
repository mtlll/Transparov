use std::{io, panic};
use std::fs;
use std::io::{Error, stdin};
use std::path::Path;

use log::{info, SetLoggerError};
use simplelog::{Config, LevelFilter, WriteLogger};
use vampirc_uci::parse_one;

use engine::Engine;

mod engine;

enum EngineError {
    IOError(io::Error),
    LoggerError(SetLoggerError)
}
impl From<io::Error> for EngineError {
    fn from(err: Error) -> Self {
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
    let _ = WriteLogger::init(LevelFilter::Info,
                              Config::default(),
                              logfile)?;
    panic::set_hook(Box::new(|panic_info| {
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            if let Some(loc) = panic_info.location() {
                info!("panic occurred in {} at line {}: {:?}", loc.file(), loc.line(), s);
            }

            else {
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
    let (handle, tx) = {
        let engine = Engine::default();
        engine.start()
    };

    loop {
        stdin().read_line(&mut input)?;

        let message = parse_one(&input);
        info!("rx: {:?}", message);

        if tx.send(message).is_err() {
            break;
        }

        input.clear();

    }
    handle.join();
    Ok(())
}