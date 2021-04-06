use stakker::{ret, ret_do, ret_nop, Ret, Stakker};
use stakker_async_await::*;
use std::cell::Cell;
use std::rc::Rc;

pub fn test_spawn(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 123456;

    spawn_future(
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

pub fn test_future_push_result(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    const VAL: u32 = 654321;

    let (fut, ret) = ret_to_future_result::<u32>(s);

    spawn_future(
        s,
        async { fut.await.expect("`ret` was unexpectedly dropped") },
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

pub fn test_future_push_result_actorfail(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    let (fut, ret) = ret_to_future_result::<u32>(s);

    spawn_future(
        s,
        async move {
            assert!(matches!(fut.await, Err(ActorFail)));
            okay.set(true);
            0
        },
        ret_nop!(),
    );

    // First run causes it to advance to the await and yield
    s.run(s.now(), false);
    assert_eq!(okay0.get(), false);

    // Second run, drop the `ret`, causing a None to be passed
    drop(ret);
    s.run(s.now(), false);
}

pub fn test_future_push(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    const VAL: u32 = 132435;

    let (fut, ret) = ret_to_future::<u32>(s);

    spawn_future(
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

pub fn test_future_push_actorfail(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let okay0 = okay.clone();
    let (fut, ret) = ret_to_future::<u32>(s);

    spawn_future(
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

pub fn test_future_pull_result(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 534231;

    let ret = ret_do!(|m: Option<Ret<u32>>| {
        if let Some(r) = m {
            ret!([r], VAL);
        }
    });
    let fut = future_pull_result(s, ret);

    spawn_future(
        s,
        async { fut.await.expect("`ret` was unexpectedly dropped") },
        ret_do!(move |v| {
            assert_eq!(v, Some(VAL));
            okay.set(true);
        }),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}

pub fn test_future_pull_result_actorfail(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let ret = ret_do!(|_: Option<Ret<u32>>| {
        // Just drop it to simulate an actor failing
    });
    let fut = future_pull_result(s, ret);

    spawn_future(
        s,
        async move {
            assert!(matches!(fut.await, Err(ActorFail)));
            okay.set(true);
            0
        },
        ret_nop!(),
    );

    // The whole thing should complete in a single run call
    s.run(s.now(), false);
}

pub fn test_future_pull(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    const VAL: u32 = 534231;

    let ret = ret_do!(|m: Option<Ret<u32>>| {
        if let Some(r) = m {
            ret!([r], VAL);
        }
    });
    let fut = future_pull(s, ret);

    spawn_future(
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

pub fn test_future_pull_actorfail(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let ret = ret_do!(|_: Option<Ret<u32>>| {
        // Just drop it to simulate an actor failing
    });
    let fut = future_pull(s, ret);

    spawn_future(
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
