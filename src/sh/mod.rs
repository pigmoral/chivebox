pub mod rush;

use crate::applets::AppletArgs;

pub fn main(_args: AppletArgs) -> i32 {
    rush::run_shell()
}
