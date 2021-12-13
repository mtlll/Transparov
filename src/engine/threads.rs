use super::search;
use crate::engine::eval::Eval;
use crate::engine::ttable::{EntryType, EvalMove, TT};
use chess::{Board, BoardStatus, ChessMove, MoveGen};
use log::info;
use num_cpus;
use std::cell::{Cell, Ref, RefCell};
use std::cmp::{max, min, Reverse};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::thread::JoinHandle;

use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use vampirc_uci::{UciInfoAttribute, UciMessage};

pub struct _WorkerThread {
    pub root_data: Mutex<RootData>,
    lock: Mutex<bool>,
    cv: Condvar,
    tx: SyncSender<UciMessage>,
    exit: AtomicBool,
    searching: AtomicBool,
    is_main: bool,
}

pub struct Worker {
    pub data: WorkerThread,
    pub handle: JoinHandle<()>,
}
pub type WorkerThread = Arc<_WorkerThread>;
impl Worker {
    pub fn new(is_main: bool, tx: SyncSender<UciMessage>) -> Self {
        let data = _WorkerThread::new(is_main, tx);
        let arc = data.clone();
        let handle = thread::spawn(move || {
            arc.idle();
        });

        Worker { data, handle }
    }
}

#[derive(Default)]
pub struct RootData {
    board: Board,
    moves: Vec<EvalMove>,
    pv: Vec<ChessMove>,
    best_move: Option<EvalMove>,
    root_depth: u8,
    completed_depth: u8,
}

impl RootData {
    pub fn clear(&mut self) {
        // self.moves.clear();
        // self.pv.clear();
        self.root_depth = 0;
        self.completed_depth = 0;
    }

    pub fn populate(&mut self, board: &Board) {
        self.clear();
        self.board = *board;
        self.moves
            .extend(MoveGen::new_legal(board).map(|mv| EvalMove {
                mv,
                eval: -search::SCORE_INF,
            }));
    }

    pub fn root_search(&mut self, mut alpha: Eval, mut beta: Eval, depth: u8) -> Eval {
        match self.board.status() {
            BoardStatus::Checkmate => {
                return -search::SCORE_MATE;
            }
            BoardStatus::Stalemate => {
                return 0;
            }
            _ => {}
        }

        let mating_score = search::SCORE_MATE;

        let mut max = -search::SCORE_INF;

        let (table_entry, handle) = TT.probe(&self.board);
        let mut best_move = None;
        let mut tt_move: Option<ChessMove> = None;
        let mut tt_depth: u8 = 0;
        let mut tt_eval: Eval = -search::SCORE_INF;

        if let Some(te) = table_entry {
            tt_move = Some(te.mv.into());
            tt_depth = te.depth;
            tt_eval = te.eval;
        }

        for em in self.moves.iter_mut() {
            if THREADS.stopped() {
                return 0;
            }

            let EvalMove { mv, eval } = em;
            let pos = if let Some(new_pos) = search::make_move_new(&self.board, *mv).take() {
                new_pos
            } else {
                if tt_move == Some(*mv) {
                    info!("Attempted move came from the TT");
                } else {
                    info!("Attempted move did not come from the TT");
                }
                continue;
            };

            /* If it's the principal variation, do a full search.
             * Otherwise, do a null window search to see if
             * an improvement is possible.
             * If the position is previously unseen, do a regular alpha/beta search.
             */

            let value = if tt_move == Some(*mv) && tt_depth >= depth {
                tt_eval
            } else {
                -search::alphabeta(pos, -beta, -alpha, depth - 1, 1)
            };

            assert!(value > -search::SCORE_INF && value < search::SCORE_INF);

            if THREADS.stopped() {
                return 0;
            }
            if value > alpha {
                *eval = value;
            } else {
                *eval = -search::SCORE_INF;
            }

            if value >= beta {
                TT.save(handle, &self.board, *mv, value, depth, EntryType::Cut);
                return value;
                //return search::quiesce(board, alpha, beta);
            }

            if value > max {
                max = value;
                best_move = Some(EvalMove::new(*mv, value));
                if value > alpha {
                    if value < beta {
                        alpha = value;
                    } else {
                        break;
                    }
                }
            } else {
                *eval = -search::SCORE_INF;
            }

            //mate pruning
            if mating_score < beta {
                beta = mating_score;
                if alpha >= mating_score {
                    return mating_score;
                }
            }

            if -mating_score > alpha {
                alpha = -mating_score;
                if beta <= -mating_score {
                    return -mating_score;
                }
            }
        }

        if let Some(EvalMove { mv, eval }) = best_move {
            let entry_type = if max < alpha {
                EntryType::All
            } else {
                EntryType::Pv
            };

            TT.save(handle, &self.board, mv, eval, depth, entry_type);
        }

        if max >= search::SCORE_MATE - depth as Eval {
            max - 1
        } else if max < -search::SCORE_MATE + depth as Eval {
            max + 1
        } else {
            max
        }
    }
}

impl _WorkerThread {
    pub fn new(is_main: bool, tx: SyncSender<UciMessage>) -> WorkerThread {
        Arc::new(_WorkerThread {
            root_data: Mutex::default(),
            lock: Mutex::new(false),
            cv: Condvar::new(),
            tx,
            exit: AtomicBool::new(false),
            searching: AtomicBool::new(true),
            is_main,
        })
    }

    pub fn idle(&self) {
        loop {
            info!("Entering idle loop...");
            {
                let lock = self.lock.lock().unwrap();
                self.searching.store(false, Ordering::Relaxed);
                self.cv.notify_one();

                self.cv.wait_while(lock, |_| {
                    !(self.searching.load(Ordering::Acquire) || self.exit.load(Ordering::Acquire))
                });

                if self.exit.load(Ordering::Acquire) {
                    info!("Worker thread exiting...");
                    return;
                }
            }

            {
                info!("Woken up, starting search...");
                let lock = self.root_data.lock().unwrap();
                self.search(lock);
            }
            if self.is_main {
                info!("Electing best move...");
                let best_move = THREADS.elect_best_move();

                info!(
                    "sending final best move({}) to engine controller...",
                    best_move
                );
                self.tx.send(UciMessage::best_move(best_move));
            }
        }
    }

    pub fn search(&self, mut data: MutexGuard<RootData>) {
        if self.is_main {
            info!("Waking slave threads...");
            if !data.moves.is_empty() {
                THREADS.start_search();
            }
        }

        let mut alpha: Eval = -search::SCORE_INF;
        let mut delta = alpha;
        let mut best_value = alpha;
        let mut beta = search::SCORE_INF;

        let mut depth = data.root_depth;
        let mut failed_high_count: u8 = 0;

        while depth < 255 && !THREADS.stopped() {
            if depth >= 4 {
                let prev = data
                    .best_move
                    .map(|EvalMove { mv, eval }| eval)
                    .unwrap_or(data.moves[0].eval);
                delta = 17 + prev * (prev / 16384);
                alpha = max(prev.saturating_sub(delta), -search::SCORE_INF);
                beta = min(prev + delta, search::SCORE_INF);
            }

            loop {
                let adj_depth = max(1, depth.saturating_sub(failed_high_count));
                best_value = data.root_search(alpha, beta, adj_depth);

                data.moves.sort_by_key(|&em| Reverse(em));

                if THREADS.stopped() {
                    break;
                }

                if best_value <= alpha {
                    beta = alpha + beta / 2;
                    alpha = max(best_value.saturating_sub(delta), -search::SCORE_INF);
                    failed_high_count = 0;
                } else if best_value >= beta {
                    beta = min(best_value.saturating_add(delta), search::SCORE_INF);
                    failed_high_count += 1;
                } else {
                    break;
                }

                delta = delta.saturating_add((delta / 4) + 5);
            }
            if !THREADS.stopped() {
                data.completed_depth = data.root_depth;
                data.best_move = Some(data.moves[0]);
                if self.is_main {
                    let bm = data.moves[0].mv;
                    info!("sending best move so far({}) to engine controller...", bm);
                    self.tx
                        .send(make_info_message(data.moves[0], data.completed_depth));
                }
            } else {
                data.moves.sort_by_key(|&em| Reverse(em));
            }

            data.root_depth += 1;
            depth = data.root_depth;
        }

        if !self.is_main {
            return;
        }

        info!("waiting for search to be stopped...");
        while !THREADS.stopped() {}

        THREADS.stop();
        info!("waiting for slave threads to go idle...");

        THREADS.wait();
    }
}

fn make_info_message(best_move: EvalMove, depth: u8) -> UciMessage {
    use UciInfoAttribute::*;
    use UciMessage::*;

    Info(vec![
        Pv(vec![best_move.mv]), //TODO: keep track of Principal Variation.
        Depth(depth),
        //TODO: report mating scores correctly
        UciInfoAttribute::from_centipawns(best_move.eval as i32),
    ])
}

impl Worker {
    pub fn start_search(&self) {
        let data = &self.data;
        let lock = data.lock.lock().unwrap();
        data.searching.store(true, Ordering::Release);
        data.cv.notify_one();
    }

    pub fn wait(&self) {
        let data = &self.data;
        let lock = data.lock.lock().unwrap();
        data.cv
            .wait_while(lock, |_| data.searching.load(Ordering::Acquire));
    }

    pub fn clear(&mut self) {}

    pub fn populate(&self, board: &Board) {
        let mut lock = self.data.root_data.lock().unwrap();
        lock.populate(board);
    }

    pub fn die(self) {
        self.data.exit.store(true, Ordering::Release);
        self.data.cv.notify_one();
        self.handle.join();
    }

    pub fn vote(&self) -> Option<(ChessMove, Eval, u8)> {
        info!("Waiting on root_data lock...");
        let lock = self.data.root_data.lock().unwrap();
        lock.best_move
            .map(|EvalMove { mv, eval }| (mv, eval, lock.completed_depth))
    }
}

pub struct ThreadPool {
    workers: RefCell<Vec<Worker>>,
    nworkers: Cell<usize>,
    stop: AtomicBool,
}

unsafe impl Sync for ThreadPool {}

impl ThreadPool {
    pub const fn new() -> Self {
        let stop = AtomicBool::new(false);
        let mut workers = RefCell::new(Vec::new());

        ThreadPool {
            workers,
            nworkers: Cell::new(0),
            stop,
        }
    }

    pub fn init(&self, tx: SyncSender<UciMessage>) {
        let nworkers = num_cpus::get();
        //let nworkers = 1;

        assert!(nworkers > 0);
        self.nworkers.set(nworkers);
        let mut workers = self.workers.borrow_mut();

        for i in 0..nworkers {
            workers.push(Worker::new(i == 0, tx.clone()));
        }
    }

    pub fn start_thinking(&self, board: &Board) {
        self.main().wait();
        self.stop.store(false, Ordering::Release);

        for worker in self.workers().iter() {
            worker.populate(board);
        }

        self.main().start_search();
    }

    pub fn start_search(&self) {
        for i in 1..self.nworkers() {
            self.workers()[i].start_search();
        }
    }

    pub fn wait(&self) {
        for i in 1..self.nworkers() {
            self.workers()[i].wait();
        }
    }

    pub fn elect_best_move(&self) -> ChessMove {
        let votes: Vec<_> = self.workers().iter().filter_map(Worker::vote).collect();
        let min_score = votes.iter().map(|(_, score, _)| score).min().unwrap();
        let mut election: HashMap<ChessMove, i32> = HashMap::new();

        votes.iter().for_each(|&(mv, score, depth)| {
            info!("move: {}, score: {}, depth: {}", mv, score, depth);
            let value: i32 = (score - min_score + 14) as i32 * depth as i32;
            election
                .entry(mv)
                .and_modify(|v| *v += value)
                .or_insert(value);
        });

        info!("Election results:");

        election.iter().for_each(|(mv, eval)| {
            info!("{}: {}", mv, eval);
        });

        election.drain().max_by_key(|&(_, v)| v).unwrap().0
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn quit(&self) {
        self.stop();
        self.wait();
        let mut workers = self.workers.borrow_mut();
        for worker in workers.drain(..) {
            worker.die();
        }
    }
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn workers(&self) -> Ref<Vec<Worker>> {
        self.workers.borrow()
    }

    fn nworkers(&self) -> usize {
        self.nworkers.get()
    }

    fn main(&self) -> Ref<Worker> {
        let workers = self.workers();

        Ref::map(workers, |w| unsafe { w.get_unchecked(0) })
    }
}

pub static THREADS: ThreadPool = ThreadPool::new();
