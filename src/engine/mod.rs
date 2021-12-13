use chess::{Board, ChessMove, Color, Game};
use std::str::FromStr;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use vampirc_uci::{UciInfoAttribute, UciMessage, UciTimeControl};

use log::info;

pub mod eval;
pub mod search;
pub mod threads;
pub mod ttable;

use crate::engine::threads::THREADS;

struct SearchHandle {
    start_time: Instant,
    search_length: Option<Duration>,
}

pub struct Engine {
    board: Option<Board>,
    best_move: Option<ChessMove>,
    channel_tx: SyncSender<UciMessage>,
    channel_rx: Receiver<UciMessage>,
    searcher: Option<SearchHandle>,
}

impl SearchHandle {
    fn new(search_length: Option<Duration>) -> Self {
        let start_time = Instant::now();

        SearchHandle {
            search_length,
            start_time,
        }
    }

    fn search(&mut self, board: &Board, moves: Option<Vec<ChessMove>>, depth: Option<u8>) {
        info!(
            "Searching for {:?} at depth {:?}.",
            self.search_length, depth
        );
        THREADS.start_thinking(board);
    }

    fn search_done(&self) -> bool {
        if THREADS.stopped() {
            info!("Search completed on its own.");
            return true;
        }

        self.search_length.map_or(false, |dur| {
            if self.elapsed() >= dur {
                info!("Search is overdue. Stopping search...");
                THREADS.stop();
                true
            } else {
                false
            }
        })
    }

    fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for Engine {
    fn default() -> Self {
        let (tx, rx) = mpsc::sync_channel(128);
        Engine {
            board: None,
            best_move: None,
            channel_tx: tx,
            channel_rx: rx,
            searcher: None,
        }
    }
}

impl Engine {
    pub fn start(self) -> (JoinHandle<()>, SyncSender<UciMessage>) {
        let tx1 = self.channel_tx.clone();
        let tx2 = self.channel_tx.clone();

        threads::THREADS.init(tx1);

        (thread::spawn(|| self.run()), tx2)
    }

    fn run(mut self) {
        let timeout = Duration::from_millis(2);
        loop {
            if let Ok(message) = self.channel_rx.recv_timeout(timeout) {
                if !self.handle_message(message) {
                    info!("Returning from event loop");
                    return;
                }
            }

            if let Some(searcher) = self.searcher.as_ref() {
                if self.best_move.is_some() && searcher.search_done() {
                    self.searcher = None;
                }
            }

            //thread::yield_now()
        }
    }

    fn handle_message(&mut self, message: UciMessage) -> bool {
        info!("rx: {}", message);
        match message {
            UciMessage::Uci => {
                id();
                //option
                uciok();
            }
            UciMessage::Debug(_) => { /*ignore for now */ }
            UciMessage::IsReady => {
                readyok();
            }
            UciMessage::Register { later, name, code } => {}
            UciMessage::Position {
                startpos,
                fen,
                moves,
            } => {
                let mut game = if let Some(fen) = fen {
                    Game::from_str(fen.as_str()).unwrap()
                } else {
                    Game::new()
                };

                for mv in moves {
                    game.make_move(mv);
                }

                self.board = Some(game.current_position());
            }
            UciMessage::SetOption { .. } => {}
            UciMessage::UciNewGame => {
                //create a new game
                self.board = None;
            }
            UciMessage::Stop => {
                THREADS.stop();
                if let Some(best_move) = self.best_move.take() {
                    bestmove(best_move, None);
                }
            }

            UciMessage::PonderHit => {}
            UciMessage::Quit => {
                info!("Told to quit. Shutting down Threadpool...");
                THREADS.quit();
                info!("Threadpool shut down.");
                return false;
            }
            UciMessage::Go {
                time_control,
                search_control,
            } => {
                // start calculating
                let mut search_time: Option<Duration> = None;
                let mut depth: Option<u8> = None;
                let mut moves: Option<Vec<ChessMove>> = None;

                if let Some(sctrl) = search_control {
                    depth = sctrl.depth;
                    if !sctrl.search_moves.is_empty() {
                        moves = Some(sctrl.search_moves)
                    }
                }
                if let Some(tctrl) = time_control {
                    search_time = self
                        .board
                        .map(|board| calculate_time(tctrl, board.side_to_move()))
                        .flatten()
                }

                let mut searcher = SearchHandle::new(search_time);

                if let Some(board) = self.board.as_ref() {
                    searcher.search(board, moves, depth);
                }

                self.searcher = Some(searcher);
            }
            UciMessage::BestMove { best_move, .. } => {
                if self.best_move.is_some() {
                    info!("printing best move...");
                    println!("{}", message);
                    self.best_move = None;
                } else {
                    info!("search result received after move already reported, ignoring...");
                }
            }
            UciMessage::Info(attrs) => {
                //If the search has already stopped, we can ignore this.
                if !THREADS.stopped() {
                    self.best_move = attrs
                        .iter()
                        .filter_map(|attr| match attr {
                            UciInfoAttribute::Pv(pv) => pv.get(0).cloned(),
                            _ => None,
                        })
                        .next();
                }
            }

            _ => {}
        }

        true
    }
}

fn calculate_time(time_control: UciTimeControl, to_move: Color) -> Option<Duration> {
    match time_control {
        UciTimeControl::MoveTime(duration) => duration.to_std().ok(),
        UciTimeControl::TimeLeft {
            white_time,
            black_time,
            moves_to_go,
            ..
        } => {
            match to_move {
                Color::White => white_time,
                Color::Black => black_time,
            }
            .map(|d| {
                //Convert from vampirc Duration to std duration.
                d.to_std().ok()
            })
            .flatten()
            .map(|d| {
                //Divide by moves until next time control or some sensible default
                d.div_f32(moves_to_go.unwrap_or(40) as f32)
            })
        }
        _ => None,
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
    reply(UciMessage::BestMove { best_move, ponder });
}
