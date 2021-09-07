use chess::{Board, ChessMove, MoveGen, Color, BitBoard};
use super::eval;

use log::{info, trace};
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::ops::{Range, Deref};
use std::iter;
use std::sync::Arc;
use std::sync::atomic;
use std::sync::mpsc::Sender;

use vampirc_uci::UciMessage;
use std::cmp::{Ordering, Reverse};

#[derive(Eq, Clone, Copy, Debug)]
pub struct EvalMove {
    pub mv: ChessMove,
    pub eval: i32,
    pub depth: u8,
}

impl EvalMove {
    fn new(mv: ChessMove, eval: i32, depth: u8) -> Self {
        EvalMove {
            mv,
            eval,
            depth,
        }
    }
}

impl PartialEq for EvalMove {
    fn eq(&self, other: &Self) -> bool {
        self.eval == other.eval && self.depth == other.depth
    }
}

impl PartialOrd for EvalMove {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvalMove {
    fn cmp(&self, other: &Self) -> Ordering {
        let eval_cmp = self.eval.cmp(&other.eval);

        match eval_cmp {
            Ordering::Equal => self.depth.cmp(&other.depth),
            _ => eval_cmp
        }
    }
}

struct TableEntry {
    best_move: EvalMove,
    rest: Vec<ChessMove>,
}

static SCORE_MATE: i32 = 1000000;
pub fn search(board: Board,
              moves: Option<Vec<ChessMove>>,
              depth: Option<u8>,
              stop: Arc<atomic::AtomicBool>,
              tx: Sender<UciMessage>) {

    let mut pos_table: HashMap<Board,TableEntry> = HashMap::new();

    let mut bestmove : Option<EvalMove> = None;


    let mut move_order : BinaryHeap<Reverse<EvalMove>> = BinaryHeap::new();
    let range = if let Some(depth) = depth {
        0..=depth - 1
    } else {
        0..=255
    };

    let mut max = -9999999;

    for depth in range {
        let mut alpha = -1000000;
        let mut beta = 1000000;

        let mut to_eval: Vec<EvalMove> = if (depth > 0) {
            let mut ret = move_order.into_sorted_vec();
            move_order = BinaryHeap::new();
            ret.iter().map(|e| e.0).collect()
        } else if let Some(moves) = moves.as_ref() {
            moves.clone().iter().map(|mv| EvalMove::new(*mv, 0, 0)).collect()
        } else {
            order_moves(&board).iter().map(|mv| EvalMove::new(*mv, 0, 0)).collect()
        };

        info!("searching: depth {}, {} legal moves", depth, to_eval.len());

        if let Some(principal) = bestmove {
           let pos = board.make_move_new(principal.mv);
            let eval = -alphabeta(pos, -beta, -alpha, depth);
            info!("eval {}: {}(depth {})", principal.mv, eval, depth);
            max = eval;
            bestmove = Some(EvalMove::new(principal.mv, eval, depth))
        }

        for EvalMove {mv, eval, ..} in to_eval.drain(..) {
            if stop.load(atomic::Ordering::Acquire) {
                break;
            }

            let pos = board.make_move_new(mv);

            if depth == 0 || -alphabeta(pos.clone(), -max - 1, -max, depth) > max {
                let eval = -alphabeta(pos, -1000000, 1000000, depth);
                info!("eval {}: {}(depth {})", mv, eval, depth);

                if eval > max {
                    info!("new best move: {}", mv);
                    max = eval;
                    if let Some(old_move) = bestmove {
                        move_order.push(Reverse(old_move));
                    }
                    bestmove = Some(EvalMove::new(mv, eval, depth));
                } else {
                    move_order.push(Reverse(EvalMove::new(mv, eval, depth)));
                }
            } else {
                move_order.push(Reverse(EvalMove::new(mv, eval, depth)));
            }
        }


        if stop.load(atomic::Ordering::Acquire) {
            break;
        } else if let Err(_) = tx.send(UciMessage::best_move(bestmove.unwrap().mv)) {
            break;
        }
    }

    stop.store(true, atomic::Ordering::Release);
}

fn order_moves(board: &Board) -> Vec<ChessMove> {
    let mut captures = MoveGen::new_legal(board);
    let mut other = MoveGen::new_legal(board);

    let occupied = *board.color_combined(!board.side_to_move());
    let empty = !*board.combined();

    captures.set_iterator_mask(occupied);
    other.set_iterator_mask(empty);

    captures.chain(other).collect()

}
fn alphabeta(board: Board, mut alpha: i32, beta: i32, depth: u8) -> i32 {
    trace!("a/b alpha: {}, beta: {}, depth: {}", alpha, beta, depth);
    if depth == 0 {
        return quiesce(board, alpha, beta);
        //return eval::evaluate_board(&board);
    }

    let mut max = -9999999;

    let legal = order_moves(&board);
    if board.checkers().popcnt() > 0 && legal.len() == 0 {
        return -SCORE_MATE;
    }

    for mv in legal {
        let score = -alphabeta(board.make_move_new(mv), -beta, -alpha, depth - 1);

        //info!("{}eval {}: {}(depth {})", indentation, mv, score, depth);

        if score >= beta {
            return score;
            //return quiesce(board, alpha, beta);
        }

        if score > max {
            max = score;
            if score > alpha {
                alpha = score;
            }
        }
    }

    if max > 100000 {
        max - 1
    } else if max < - 100000 {
        max + 1
    } else {
        max
    }
}
static DELTA_MARGIN: i32 = 200;
fn quiesce(board: Board, mut alpha: i32, beta: i32) -> i32 {
    let cur_eval = eval::evaluate_board(&board);

    if cur_eval >= beta {
        return beta;
    } else if alpha < cur_eval {
        alpha = cur_eval;
    }

    let min_color = match board.side_to_move() {
        Color::White => Color::Black,
        Color::Black => Color::White,
    };

    let mut max = -9999999;
    let mut legal = MoveGen::new_legal(&board);
    legal.set_iterator_mask(*board.color_combined(min_color));

    for mv in legal {
        let piece = board.piece_on(mv.get_dest()).unwrap();
        if cur_eval + eval::PIECE_VALUES[piece.to_index()] + DELTA_MARGIN < alpha {
            continue;
        }
        let score = -quiesce(board.make_move_new(mv), -beta, -alpha);

        if score >= beta {
            return score;
        } else if score > max {
            max = score;
            if score > alpha {
                alpha = score;
            }
        }
    }

    alpha
}
fn negamax(board: Board, depth: u8) -> i32 {
    if depth == 0 {
        return eval::evaluate_board(&board);
    }

    let mut max: i32 = -9999;

    for mv in MoveGen::new_legal(&board) {
        let score = -negamax(board.make_move_new(mv), depth - 1);
        if score > max {
            max = score;
        }
    }

    max
}

#[cfg(test)]
mod tests {
    #[macro_use]
    use more_asserts::*;
    use super::EvalMove;
    use super::ChessMove;
    use std::collections::BinaryHeap;
    use std::cmp::Reverse;

    #[test]
    fn move_ordering() {
        let mv = ChessMove::default();
        assert_gt!(
            EvalMove::new(mv, 100, 0),
            EvalMove::new(mv, -100, 0)
        );
        assert_gt!(
            EvalMove::new(mv, 100, 1),
            EvalMove::new(mv, 100, 0)
        );
        assert_eq!(
            EvalMove::new(mv, 100, 0),
            EvalMove::new(mv, 100, 0)
        );
        assert_gt!(
            EvalMove::new(mv, 200, 1),
            EvalMove::new(mv, 100, 3)
        );

        let mut test_heap: BinaryHeap<Reverse<EvalMove>> = BinaryHeap::new();
        test_heap.push(Reverse(EvalMove::new(mv, 500, 0)));
        test_heap.push(Reverse(EvalMove::new(mv, 400, 0)));
        test_heap.push(Reverse(EvalMove::new(mv, -100, 0)));
        test_heap.push(Reverse(EvalMove::new(mv, -300, 0)));

        let test_vec = test_heap.clone().into_sorted_vec();

        assert_eq!(test_heap.pop().unwrap().0.eval, test_vec[3].0.eval);
        assert_eq!(test_heap.pop().unwrap().0.eval, test_vec[2].0.eval);
        assert_eq!(test_heap.pop().unwrap().0.eval, test_vec[1].0.eval);
        assert_eq!(test_heap.pop().unwrap().0.eval, test_vec[0].0.eval);
    }
}