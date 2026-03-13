pub mod rush;

use std::env;
use std::iter;

pub fn main(_args: iter::Skip<env::ArgsOs>) -> i32 {
    rush::run_shell()
}
