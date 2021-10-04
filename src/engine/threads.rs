use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use num_cpus;
use std::thread;
use chess::Board;

pub struct WorkerThread {
    pub root_data: Mutex<RootData>,
    lock: Mutex<bool>,
    cv: Condvar,
    stop: Arc<AtomicBool>,
    exit: AtomicBool,
    searching: AtomicBool,
    is_main: bool
}

#[derive(Default)]
pub struct RootData {

}

impl RootData {
    pub fn clear(&mut self) {

    }

    pub fn populate(&mut self, board: &Board) {

    }
}

impl WorkerThread {
    pub fn new(stop: Arc<AtomicBool>, is_main: bool) -> (Arc<Box<Self>>, JoinHandle<()>) {

        let worker = Box::new(WorkerThread {
            root_data: Mutex::default(),
            lock: Mutex::new(false),
            cv: Condvar::new(),
            stop,
            exit: AtomicBool::new(false),
            searching: AtomicBool::new(true),
            is_main
        });

        let arc = Arc::new(worker);

        let handle = {
            let bor = arc.clone();
            thread::spawn(move || bor.idle())
        };

        (arc, handle)
    }

    pub fn start_search(&self) {
        let lock = self.lock.lock().unwrap();
        self.searching.store(true, Ordering::SeqCst);
        self.cv.notify_one();
    }

    pub fn idle(&self) {
        loop {
            {
                let lock = self.lock.lock().unwrap();
                self.searching.store(false, Ordering::SeqCst);
                self.cv.notify_one();

                self.cv.wait_while(lock, |_| !self.searching.load(Ordering::SeqCst));

                if self.exit.load(Ordering::SeqCst) {
                    return;
                }
            }

            self.search(self.root_data.lock().unwrap());
        }
    }

    pub fn wait(&self) {
        let lock = self.lock.lock().unwrap();
        self.cv.wait_while(lock, |_| self.searching.load(Ordering::SeqCst));
    }

    pub fn clear(&mut self) {

    }

    pub fn populate(&self, board: &Board) {
        let mut lock = self.root_data.lock().unwrap();
        lock.populate(board);
    }

    pub fn search(&self, root_data: MutexGuard<RootData>) {

    }
}

pub type Worker = (Arc<Box<WorkerThread>>, JoinHandle<()>);

pub struct ThreadPool {
    workers: Vec<Worker>,
    nworkers: usize,
    stop: Arc<AtomicBool>
}

impl ThreadPool {
    pub fn new() -> Self {
        let nworkers = num_cpus::get();
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::new();

        assert!(nworkers > 0);

        for i in 0..nworkers {
            workers.push(WorkerThread::new(stop.clone(), i == 0));
        }

        ThreadPool {
            workers,
            nworkers,
            stop
        }
    }

    pub fn start_thinking(&self, board: &Board) {
        self.main().0.wait();
        self.stop.store(false, Ordering::SeqCst);

        for (worker, _) in &self.workers {
            worker.populate(board);
        }

        self.main().0.start_search();
    }
    pub fn start_search(&self) {
        for i in 1..self.nworkers {
            self.workers[i].0.start_search();
        }
    }

    pub fn wait(&self) {
        for i in 1..self.nworkers {
            self.workers[i].0.wait();
        }
    }

    fn main(&self) -> &Worker {
        unsafe {
            self.workers.get_unchecked(0)
        }
    }


}