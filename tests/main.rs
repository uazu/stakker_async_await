use stakker::Stakker;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

mod futures_core;
mod rust_std;

use crate::futures_core::*;
use rust_std::*;

// Due to the default test harness forcing everything to run in
// parallel, and cargo not allowing us to enable the multi-thread
// feature for Stakker just for tests (without affecting the main
// build), instead we have to run our own minimal single-threaded test
// harness.

fn main() {
    let mut pass = 0;
    let mut fail = 0;

    macro_rules! run {
        ($name:ident) => {
            run_test(stringify!($name), |s, o| $name(s, o), &mut pass, &mut fail);
        };
    }

    println!("Running stakker_async_await tests:");
    run!(test_spawn);
    run!(test_future_push);
    run!(test_future_push_actorfail);
    run!(test_future_push_result);
    run!(test_future_push_result_actorfail);
    run!(test_future_pull);
    run!(test_future_pull_actorfail);
    run!(test_future_pull_result);
    run!(test_future_pull_result_actorfail);
    run!(test_stream_preload);
    run!(test_stream_batches);
    run!(test_stream_result_preload);
    run!(test_stream_result_batches);
    run!(test_stream_result_actorfail);

    if fail != 0 {
        println!("test result: FAILED.  {} passed, {} failed", pass, fail);
        std::process::exit(1);
    }
    println!("test result: ok.  {} passed, {} failed", pass, fail);
    println!();
}

fn run_test(
    name: &str,
    run: impl FnOnce(&mut Stakker, Rc<Cell<bool>>),
    pass: &mut u32,
    fail: &mut u32,
) {
    print!("- {} ... ", name);
    let okay = Rc::new(Cell::new(false));
    let okay2 = okay.clone();
    let now = Instant::now();
    let mut stakker = Stakker::new(now);
    let s = &mut stakker;
    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || run(s, okay2))) {
        let msg = match e.downcast::<String>() {
            Ok(v) => *v,
            Err(e) => match e.downcast::<&str>() {
                Ok(v) => v.to_string(),
                Err(e) => format!("Panic with unknown type: {:?}", e.type_id()),
            },
        };
        println!("FAILED");
        println!("    {}", msg);
        *fail += 1;
    } else if !okay.get() {
        println!("FAILED");
        println!("    `okay` not set to true");
        *fail += 1;
    } else {
        println!("OK");
        *pass += 1;
    }
}
