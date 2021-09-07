mod engine;
use engine::search;
use engine::search::EvalMove;
use engine::Engine;


use std::process::exit;
use std::io;
use std::io::{stdout, stdin, Write, Error};
use vampirc_uci::{UciMessage, parse_one, uci::UciFen};
use std::str::FromStr;
use std::fs;
use std::path::Path;
use std::thread::JoinHandle;
use std::sync::mpsc::Sender;

use log::{info, SetLoggerError};
use simplelog::{WriteLogger, LevelFilter, Config};

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

    Ok(())
}


static DEFAULT_DEPTH: u8 = 6;
fn uci_loop() -> io::Result<()> {
    let mut input = String::new();
    let (handle, tx) = {
        let mut engine = Engine::default();
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