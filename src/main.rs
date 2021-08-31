mod engine;
use engine::search;
use engine::search::EvalMove;

use chess::Board;
use chess::MoveGen;
use chess::Color;
use chess::Game;
use chess::ChessMove;
use chess::Piece;

use std::process::exit;
use std::io;
use std::io::{stdout, stdin, Write, Error};
use vampirc_uci::{UciMessage, parse_one, uci::UciFen};
use std::str::FromStr;
use std::fs;
use std::path::Path;

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
struct Engine {
    pub game: Game,
}

impl Default for Engine {
    fn default() -> Self {
        Engine {
            game: Game::new()
        }
    }
}

impl Engine {
    pub fn new(game: Game) -> Self {
        Engine {
            game,
        }
    }
}

static DEFAULT_DEPTH: u8 = 6;
fn uci_loop() -> io::Result<()> {
    let mut input = String::new();
    let mut engine = Engine::default();

    loop {
        stdin().read_line(&mut input)?;
        info!("rxraw: {}", input);
        let message = parse_one(&input);
        info!("rx: {:?}", message);

        match message {
            UciMessage::Uci => {
                id();
                //option
                uciok();
            }
            UciMessage::Debug(_) => { /*ignore for now */}
            UciMessage::IsReady => {
                readyok();
            }
            UciMessage::Register { later, name, code} => {}
            UciMessage::Position { startpos, fen, moves} => {
                if !startpos {
                    engine = Engine::new(Game::from_str(fen.unwrap().as_str()).unwrap());
                } else {
                    engine = Engine::new(Game::new());
                }

                for mv in moves {
                    engine.game.make_move(mv);
                }
            }
            UciMessage::SetOption { .. } => {}
            UciMessage::UciNewGame => {
                //create a new game
            }
            UciMessage::Stop => {}
            UciMessage::PonderHit => {}
            UciMessage::Quit => {
                return Ok(());
            }
            UciMessage::Go { time_control, search_control } => {
                // start calculating
                let mut depth = DEFAULT_DEPTH;
                let mut moves: Option<Vec<ChessMove>> = None;
                if let Some(sctrl) = search_control {
                    depth = sctrl.depth.unwrap_or(DEFAULT_DEPTH);
                    moves = Some(sctrl.search_moves);

                }
                let eval_move = search::search(
                    &engine.game.current_position(),
                    moves,
                    depth);

                bestmove(eval_move.mv, None);
            }
            _ => {}
        }
        input.clear();

    }
}

fn id() {

    reply(UciMessage::Id {
        name: Some("Transparov".to_string()),
        author: None,
    });
    reply(UciMessage::Id {
        name: None,
        author: Some("Audun Hoem".to_string()),
    });
}

fn reply(message: UciMessage) {
    info!("tx: {:?}", message);
    println!("{}", message);
}

fn uciok() {
    reply(UciMessage::UciOk);
}

fn readyok() {
    reply(UciMessage::ReadyOk);
}

fn bestmove(best_move: ChessMove, ponder: Option<ChessMove>) {
   reply(UciMessage::BestMove {
       best_move,
       ponder
   });
}
fn prompt(prompt: &str) -> String {
    let mut s = String::new();
    print!("{}", prompt);
    let _ = stdout().flush();
    stdin().read_line(&mut s).unwrap();
    //println!("{}", s);
    s.trim().to_string()
}

fn get_color() -> Color {
    loop {
        let reply = prompt("Choose a color(white/black/random): ").to_lowercase();
        match reply.as_str() {
            "white" => {
                return Color::Black;
            },
            "black" => {
                return Color::White;
            },
            _ => {
                println!("Invalid choice");
            },
        }
    }
}

fn get_move(game: &Game) -> ChessMove {
    loop {
        let reply = prompt("Make a move: ");
        match ChessMove::from_san(&game.current_position(), &reply) {
            Ok(mv) => {
                return mv;
            },
            Err(_) => {
                println!("Invalid move: {}", reply);
            }
        }
    }

}




fn move_to_san(mv: &ChessMove, board: &Board) -> String {
    let src = mv.get_source();
    let dst = mv.get_dest();

    String::new()
}