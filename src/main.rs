use chess::Board;
use chess::MoveGen;
use chess::Color;
use chess::Game;
use chess::ChessMove;
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
            let my_move = random_legal_move(&game.current_position(), &mut rng);
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
