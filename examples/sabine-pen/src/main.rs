use sabine::prelude::*;

fn main() {
    SabineWindow::main(|window| Ok(window.app().size(1100, 720).min_size(640, 420)));
}
