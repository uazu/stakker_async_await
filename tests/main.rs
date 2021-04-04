use stakker::{ret, ret_do, ret_nop, Ret, Stakker};
use stakker_async_await::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

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
    run!(test_future_push_drop_ret);
    run!(test_future_push_some);
    run!(test_future_push_some_drop_ret);
    run!(test_future_pull);
    run!(test_future_pull_drop_ret);
    run!(test_future_pull_some);
    run!(test_future_pull_some_drop_ret);

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

//------------------------------------------------------------------------

// Test plain spawn without a yield
fn test_spawn(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 123456;

    spawn(
        s,
        async { VAL },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // First run, future is polled and should resolve immediately
    s.run(s.now(), false);
}

// Test spawn which yields, and passing value on a "push pipe"
fn test_future_push(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    const VAL: u32 = 654321;

    let (fut, ret) = future_push::<u32>(s);

    spawn(
        s,
        async { fut.await.expect("`ret` was dropped") },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // First run causes it to advance to the await and yield
    s.run(s.now(), false);
    assert_eq!(okay0.get(), false);

    // Second run, provide it with a value and let the future complete
    ret!([ret], VAL);
    s.run(s.now(), false);
}

// Test passing None on a "push pipe" by dropping the `ret`
fn test_future_push_drop_ret(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    let (fut, ret) = future_push::<u32>(s);

    spawn(
        s,
        async move {
            assert_eq!(fut.await, None);
            okay.set(true);
            0
        },
        ret_nop!(),
    );

    // First run causes it to advance to the await and yield
    s.run(s.now(), false);
    assert_eq!(okay0.get(), false);

    // Second run, drop the `ret` causing a None to be passed
    drop(ret);
    s.run(s.now(), false);
}

// Test spawn which yields, and passing value on a "push pipe"
fn test_future_push_some(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    const VAL: u32 = 132435;

    let (fut, ret) = future_push_some::<u32>(s);

    spawn(
        s,
        async { fut.await },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // First run causes it to advance to the await and yield
    s.run(s.now(), false);
    assert_eq!(okay0.get(), false);

    // Second run, provide it with a value and let the future complete
    ret!([ret], VAL);
    s.run(s.now(), false);
}

// Test passing None on a "some" push pipe by dropping the `ret`,
// which is ignored, resulting in the future being dropped before
// completion.
fn test_future_push_some_drop_ret(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    let (fut, ret) = future_push_some::<u32>(s);

    spawn(
        s,
        async {
            fut.await;
            panic!("Not expecting to resume execution after the await");
        },
        ret_do!(move |v| {
            assert_eq!(
                v, None,
                "Expecting None return from spawn due to future dying"
            );
            okay.set(true);
        }),
    );

    // First run causes it to advance to the await and yield
    s.run(s.now(), false);
    assert_eq!(okay0.get(), false);

    // Second run, drop the `ret` which causes the future to be
    // dropped
    drop(ret);
    s.run(s.now(), false);
}

// Test passing value on a "pull pipe"
fn test_future_pull(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 534231;

    let ret = ret_do!(|m: Option<Ret<u32>>| {
        if let Some(r) = m {
            ret!([r], VAL);
        }
    });
    let fut = future_pull(s, ret);

    spawn(
        s,
        async { fut.await.expect("`ret` was dropped") },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}

fn test_future_pull_drop_ret(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let ret = ret_do!(|_: Option<Ret<u32>>| {
        // Just drop it to simulate an actor failing
    });
    let fut = future_pull(s, ret);

    spawn(
        s,
        async move {
            assert_eq!(fut.await, None);
            okay.set(true);
            0
        },
        ret_nop!(),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}

// Test passing value on a "pull pipe"
fn test_future_pull_some(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 534231;

    let ret = ret_do!(|m: Option<Ret<u32>>| {
        if let Some(r) = m {
            ret!([r], VAL);
        }
    });
    let fut = future_pull_some(s, ret);

    spawn(
        s,
        async { fut.await },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}

fn test_future_pull_some_drop_ret(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let ret = ret_do!(|_: Option<Ret<u32>>| {
        // Just drop it to simulate an actor failing
    });
    let fut = future_pull_some(s, ret);

    spawn(
        s,
        async move {
            fut.await;
            panic!("Not expecting to resume execution after the await");
        },
        ret_do!(move |v| {
            assert_eq!(
                v, None,
                "Expecting None return from spawn due to future dying"
            );
            okay.set(true);
        }),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}
