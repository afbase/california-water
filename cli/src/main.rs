pub mod cmd;

use self::cmd::clap::new_app;

fn main() {
    let _matches = new_app().get_matches();
}
