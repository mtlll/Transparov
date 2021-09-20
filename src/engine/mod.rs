use chess::{Board, ChessMove, Game, Color};
use vampirc_uci;
use vampirc_uci::{UciTimeControl, UciMessage};
use std::thread;
use std::thread::JoinHandle;
use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::str::FromStr;
use std::time::{Instant, Duration};

use log::info;

pub mod search;
pub mod eval;
pub mod ttable;

use ttable::CacheTable;

struct SearchHandle {
    thread_handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    start_time: Option<Instant>,
    search_length: Option<Duration>,
}

pub struct Engine {
    board: Option<Board>,
    cache: Option<CacheTable>,
    best_move: Option<ChessMove>,
    channel_tx: Sender<UciMessage>,
    channel_rx: Receiver<UciMessage>,
    searcher: Option<SearchHandle>,

}
impl Default for SearchHandle {
    fn default() -> Self {
        SearchHandle {
            thread_handle: None,
            stop: Arc::new(AtomicBool::new(false)),
            start_time: None,
            search_length: None,
        }
    }
}

impl SearchHandle {
    fn new(search_length: Option<Duration>) -> Self {
        let mut sh = SearchHandle::default();
        sh.search_length = search_length;
        if search_length.is_some() {
            sh.start_time = Some(Instant::now());
        }

        sh
    }

    fn search(&mut self,
              board: &Board,
              cache: CacheTable,
              moves: Option<Vec<ChessMove>>,
              depth: Option<u8>,
              tx: Sender<UciMessage>) {
        if self.thread_handle.is_none() {
            info!("Searching for {:?} at depth {:?}.", self.search_length,  depth);
            let stop = self.stop.clone();
            let board = board.clone();
            let cache = cache.clone();
            self.thread_handle = Some(thread::spawn(move || {

                search::search(board, cache, moves, depth, stop, tx);
            }));
        }
    }
    fn search_done(&self) -> bool {
        if self.stop.load(Ordering::Acquire) {
            info!("Search completed on its own.");
            return true;
        }

        if let Some(duration) = self.search_length {
            if self.start_time.unwrap().elapsed() >= duration {
                info!("Search is overdue. Stopping search...");
                self.stop.store(true, Ordering::Release);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn join(&mut self) {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            handle.join();
        }
    }

    fn die(mut self) {
        thread::spawn(move || {
            self.join();
        });
    }
}

impl Default for Engine {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Engine {
            board: None,
            cache: None,
            best_move: None,
            channel_tx: tx,
            channel_rx: rx,
            searcher: None,
        }
    }
}

impl Engine {
    pub fn start(self) -> (JoinHandle<()>, Sender<UciMessage>) {
        let tx = self.channel_tx.clone();
        (thread::spawn(|| self.run()), tx)
    }

    fn run(mut self) {
        let timeout = Duration::from_millis(2);
        loop {
            if let Ok(message) = self.channel_rx.recv_timeout(timeout) {
                self.handle_message(message);
            }

            if let Some(searcher) = self.searcher.as_mut()  {
                if self.best_move.is_some() && searcher.search_done() {
                    self.searcher.take().unwrap().die();
                    if let Some(mv) = self.best_move.take() {
                        bestmove(mv, None);
                    }
                }

            }
        }
    }

    fn handle_message(&mut self, message: UciMessage) {
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
                let mut game = if let Some(fen) = fen {
                    Game::from_str(fen.as_str()).unwrap()
                } else {
                    Game::new()
                };

                for mv in moves {
                    game.make_move(mv);
                }

                self.board = Some(game.current_position());

                if self.cache.is_none() {
                    self.cache = Some(CacheTable::default());
                }
            }
            UciMessage::SetOption { .. } => {}
            UciMessage::UciNewGame => {
                //create a new game
                self.board = None;
                self.cache = None;
            }
            UciMessage::Stop => {
                if let Some(handle) = self.searcher.as_mut() {
                    handle.stop();
                }
                if let Some(best_move) = self.best_move {
                    bestmove(best_move, None);
                }
            }

            UciMessage::PonderHit => {}
            UciMessage::Quit => {
            }
            UciMessage::Go { time_control, search_control } => {
                // start calculatinr
                let mut search_time: Option<Duration> = None;
                let mut depth: Option<u8> = None;
                let mut moves: Option<Vec<ChessMove>> = None;

                if let Some(sctrl) = search_control {
                    depth = sctrl.depth;
                    if sctrl.search_moves.len() > 0 {
                        moves = Some(sctrl.search_moves)
                    }

                }
                if let Some(tctrl) = time_control {
                    search_time = self.board.map(|board| {
                        calculate_time(tctrl, board.side_to_move())
                    }).flatten()
                }

                let mut searcher = SearchHandle::new(search_time);

                if let Some(handle) = self.searcher.as_mut() {
                    handle.join();
                }

                if let Some(board) = self.board.as_ref() {
                    searcher.search(board, self.cache.as_ref().unwrap().clone(), moves, depth, self.channel_tx.clone());
                }

                self.searcher = Some(searcher);
            }
            UciMessage::BestMove {best_move, ..} => {
                self.best_move = Some(best_move);
                let mut pv = Vec::new();
                let mut bm = Some(best_move);
                let mut pos = self.board.unwrap().clone();
                let lock = self.cache.as_ref().unwrap().acquire_read();
                while bm.is_some() {
                    pv.push(bm.unwrap());
                    pos = pos.make_move_new(bm.unwrap());
                    bm = lock.get(&pos).map(|te| te.best_move.mv);
                }

                let mut pvstring = String::new();
                pv.iter().for_each(|mv| {
                    if pvstring.is_empty() {
                        pvstring = format!("{}", mv);
                    } else {
                        pvstring = format!("{},{}", pvstring, mv);
                    }
                });

                info!("Received new pv from searcher: {}.", pvstring);
            }
            _ => {}
        }
    }

}

fn calculate_time(time_control: UciTimeControl, to_move: Color) -> Option<Duration> {
    match time_control {
        UciTimeControl::MoveTime(duration) => {
            duration.to_std().ok()
        }
        UciTimeControl::TimeLeft {white_time, black_time, moves_to_go, ..} => {
            match to_move {
                Color::White => white_time,
                Color::Black => black_time,
            }.map(|d| {
                //Convert from vampirc Duration to std duration.
                d.to_std().ok()
            }).flatten().map(|d| {
                //Divide by moves until next time control or some sensible default
                d.div_f32(moves_to_go.unwrap_or(40) as f32)
            })
        }
        _ => None
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
