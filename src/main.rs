use chess::Board;
use chess::MoveGen;
use chess::Color;
use chess::Game;
use chess::ChessMove;
use chess::Piece;

use std::process::exit;
use std::io;
use rand::seq::IteratorRandom;
use rand;
use rand::Rng;
use std::io::{stdout, stdin, Write};

fn main() {
    let mut game = Game::new();
    let colour = get_color();
    let mut rng = rand::thread_rng();

    loop {
        if let Some(result) = game.result() {
            println!("{:?}", result);
            exit(0);
        } else if game.side_to_move() == colour {
           // let my_move = random_legal_move(&game.current_position(), &mut rng);
            let my_move = choose_move(&game.current_position(), &mut rng);
            game.make_move(my_move);
            println!("{}", my_move);
        } else {
            let their_move = get_move(&game);
            game.make_move(their_move);
        }
    }
}

fn random_legal_move<R: ?Sized>(board: &Board, rng: &mut R) -> ChessMove
where R: rand::Rng,
{
    let legal = MoveGen::new_legal(board);
    legal.choose(rng).unwrap()
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

fn choose_move<R: ?Sized>(board: &Board, rng: &mut R) -> ChessMove
where R: rand::Rng,
{

    let mut max: i32 = -99999;
    let mut alpha = -100000;
    let mut beta = 100000;
    let mut best_move : Option<ChessMove> = None;

    let legal = MoveGen::new_legal(board);
    let nmoves = legal.len();
    for mv in legal.choose_multiple(rng, nmoves) {
        let eval = -alphabeta(board.make_move_new(mv), -beta, -alpha,  5);
        if eval > max {
            max = eval;
            best_move = Some(mv);
        }

        if eval > alpha {
            alpha = eval;
        }
    }

    best_move.unwrap()
}

fn alphabeta(board: Board, alpha: i32, beta: i32, depth: u8) -> i32 {
    if depth == 0 {
        return evaluate_board(board);
    }

    let mut alpha = alpha;
    let mut beta = beta;

    for mv in MoveGen::new_legal(&board) {
        let score = -alphabeta(board.make_move_new(mv), -beta, -alpha, depth - 1);

        if score >= beta {
            return beta;
        } else if score > alpha {
            alpha = score;
        }
    }

    alpha
}


fn negamax(board: Board, depth: u8) -> i32 {
    if depth == 0 {
        return evaluate_board(board);
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
static PIECE_TABLES: [[[i32; 8]; 8]; 6] = [
    [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [5, 10, 10, -20, -20, 10, 10, 5],
        [5, -5, -10, 0, 0, -10, -5, 5],
        [0, 0, 0, 20, 20, 0, 0, 0],
        [5, 5, 10, 25, 25, 10, 5, 5],
        [10, 10, 20, 30, 30, 20, 10, 10],
        [50, 50, 50, 50, 50, 50, 50, 50],
        [0, 0, 0, 0, 0, 0, 0, 0]
    ],
    [
        [-50, -40, -30, -30, -30, -30, -40, -50],
        [-40, -20, 0, 5, 5, 0, -20, 40],
        [-30, 5, 10, 15, 15, 10, 5, -30],
        [-30, 0, 15, 20, 20, 15, 0, -30],
        [-30, 5, 15, 20, 20, 15, 5, -30],
        [-30, 0, 10, 15, 15, 10, 0, -30],
        [-40, 20, 0, 0, 0, 0, -20, -40],
        [-50, -40, -30, -30, -30, -30, -40, -50]
    ],
    [
        [-20, -10, -10, -10, -10, -10, -10, -20],
        [-10, 5, 0, 0, 0, 0, 5, 10],
        [-10, 10, 10, 10, 10, 10, 10, -10],
        [-10, 0, 10, 10, 10, 10, 0, -10],
        [-10, 5, 5, 10, 10, 5, 5, -10],
        [-10, 0, 5, 10, 10, 5, 0, -10],
        [-10, 0, 0, 0, 0, 0, 0, -10],
        [-20, -10, -10, -10, -10, -10, -10, -20]
    ],
    [
        [0, 0, 0, 5, 5, 0, 0, 0],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [-5, 0, 0, 0, 0, 0, 0, -5],
        [5, 10, 10, 10, 10, 10, 10, 5],
        [0, 0, 0, 0, 0, 0, 0, 0]
    ],
    [
        [-20, -10, -10, -5, -5, -10, -10, -20],
        [-10, 0, 0, 0, 0, 0, 0, -10],
        [-10, 5, 5, 5, 5, 5, 0, -10],
        [0, 0, 5, 5, 5, 5, 0, -5],
        [-5, 0, 5, 5, 5, 5, 0, -5],
        [-10, 0, 5, 5, 5, 5, 0, -10],
        [-10, 0, 0, 0, 0, 0, 0, -10],
        [-20, -10, -10, -5, -5, -10, -10, -20]
    ],
    [
        [20, 30, 10, 0, 0, 10, 30, 20],
        [20, 20, 0, 0, 0, 0, 20, 20],
        [-10, -20, -20, -20, -20, -20, -20, -10],
        [-20, -30, -30, -40, -40, -30, -30, -20],
        [-30, -40, -40, -50, -50, -40, -40, 30],
        [-30, -40, -40, -50, -50, -40, -40, 30],
        [-30, -40, -40, -50, -50, -40, -40, 30],
        [-30, -40, -40, -50, -50, -40, -40, 30]
    ]
];
fn evaluate_board(board: Board) -> i32 {
    let mut ret = count_material(board);

    ret * match board.side_to_move() {
        Color::White => 1,
        Color::Black => -1
    }
}

fn count_material(board: Board) -> i32 {
    let piece_values: [i32; 6] = [100, 300, 300, 500, 900, 0];
    let mut count: i32 = 0;
    for sq in *board.color_combined(Color::White) {
        let piece = board.piece_on(sq).unwrap().to_index();
        count += piece_values[piece];
        count += PIECE_TABLES[piece][sq.get_rank().to_index()][sq.get_file().to_index()];
    }

    for sq in *board.color_combined(Color::Black) {
        let piece = board.piece_on(sq).unwrap().to_index();
        count -= piece_values[piece];
        count -= PIECE_TABLES[piece][7 - sq.get_rank().to_index()][7 - sq.get_file().to_index()];
    }

    count
}

fn move_to_san(mv: &ChessMove, board: &Board) -> String {
    let src = mv.get_source();
    let dst = mv.get_dest();

    String::new()
}