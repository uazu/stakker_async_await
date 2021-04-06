use stakker::{fwd, fwd_do, fwd_nop, Fwd, Stakker};
use stakker_async_await::*;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

pub fn test_stream_preload(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let (strm, fwd) = fwd_to_stream(s, fwd_nop!(), 10);

    let mut expecting = (0..10).map(|v| Some(v)).collect::<VecDeque<_>>();
    expecting.push_back(None);
    for &v in &expecting {
        fwd!([fwd], v);
    }
    let expecting = RefCell::new(expecting);

    spawn_stream(
        s,
        strm,
        fwd_do!(move |v| {
            let mut expecting = expecting.borrow_mut();
            let exp = expecting
                .pop_front()
                .expect("More data received than expected");
            assert_eq!(v, exp);
            if expecting.is_empty() {
                okay.set(true);
            }
        }),
    );

    // Expect it all to go through in first run
    s.run(s.now(), false);
}

// Send numbers 0..100 in batches of 16
pub fn test_stream_batches(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let mut expecting = (0..100).map(|v| Some(v)).collect::<VecDeque<_>>();
    expecting.push_back(None);
    let tosend = RefCell::new(expecting.clone());
    let expecting = RefCell::new(expecting);

    // It seems a bit backwards here because the fwd_do doesn't have
    // the fwd available to it.  But in real life this would be going
    // via an actor.
    let actor: Rc<RefCell<Option<Fwd<Option<u32>>>>> = Rc::new(RefCell::new(None));
    let actor2 = actor.clone();
    let (strm, fwd) = fwd_to_stream(
        s,
        fwd_do!(move |()| {
            let mut tosend = tosend.borrow_mut();
            for _ in 0..16 {
                if let Some(v) = tosend.pop_front() {
                    if let Some(ref fwd) = *actor2.borrow_mut() {
                        fwd!([fwd], v);
                    }
                    if v.is_some() {
                        continue;
                    }
                    *actor2.borrow_mut() = None; // Test dropping fwd
                }
                break;
            }
        }),
        16,
    );
    *actor.borrow_mut() = Some(fwd);
    drop(actor);

    spawn_stream(
        s,
        strm,
        fwd_do!(move |v| {
            let mut expecting = expecting.borrow_mut();
            let exp = expecting
                .pop_front()
                .expect("More data received than expected");
            assert_eq!(v, exp);
            if expecting.is_empty() {
                okay.set(true);
            }
        }),
    );

    // Expect it all to go through in first run
    s.run(s.now(), false);
}

pub fn test_stream_result_preload(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let (strm, fwd) = fwd_to_stream_result(s, fwd_nop!(), 10);

    let mut expecting = (0..10).map(|v| Some(v)).collect::<VecDeque<_>>();
    expecting.push_back(None);
    for &v in &expecting {
        fwd!([fwd], v);
    }
    let expecting = RefCell::new(expecting);
    drop(fwd); // Also test dropping fwd after sending None

    spawn_stream(
        s,
        strm,
        fwd_do!(move |v| {
            let mut expecting = expecting.borrow_mut();
            let exp = expecting
                .pop_front()
                .expect("More data received than expected")
                .map(|v| Ok(v));
            assert_eq!(v, exp);
            if expecting.is_empty() {
                okay.set(true);
            }
        }),
    );

    // Expect it all to go through in first run
    s.run(s.now(), false);
}

// Send numbers 0..100 in batches of 16
pub fn test_stream_result_batches(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let mut expecting = (0..100).map(|v| Some(v)).collect::<VecDeque<_>>();
    expecting.push_back(None);
    let tosend = RefCell::new(expecting.clone());
    let expecting = RefCell::new(expecting);

    // It seems a bit backwards here because the fwd_do doesn't have
    // the fwd available to it.  But in real life this would be going
    // via an actor.
    let actor: Rc<RefCell<Option<Fwd<Option<u32>>>>> = Rc::new(RefCell::new(None));
    let actor2 = actor.clone();
    let (strm, fwd) = fwd_to_stream_result(
        s,
        fwd_do!(move |()| {
            let mut tosend = tosend.borrow_mut();
            for _ in 0..16 {
                if let Some(v) = tosend.pop_front() {
                    if let Some(ref fwd) = *actor2.borrow_mut() {
                        fwd!([fwd], v);
                    }
                    if v.is_some() {
                        continue;
                    }
                }
                break;
            }
        }),
        16,
    );
    *actor.borrow_mut() = Some(fwd);

    spawn_stream(
        s,
        strm,
        fwd_do!(move |v| {
            let mut expecting = expecting.borrow_mut();
            let exp = expecting
                .pop_front()
                .expect("More data received than expected")
                .map(|v| Ok(v));
            assert_eq!(v, exp);
            if expecting.is_empty() {
                okay.set(true);
            }
        }),
    );

    // Expect it all to go through in first run
    s.run(s.now(), false);
}

pub fn test_stream_result_actorfail(s: &mut Stakker, okay: Rc<Cell<bool>>) {
    let (strm, fwd) = fwd_to_stream_result(s, fwd_nop!(), 16);

    let mut expecting = VecDeque::new();
    expecting.push_back(Some(Ok(123)));
    expecting.push_back(Some(Err(ActorFail)));
    expecting.push_back(None);
    let expecting = RefCell::new(expecting);

    spawn_stream(
        s,
        strm,
        fwd_do!(move |v| {
            let mut expecting = expecting.borrow_mut();
            let exp = expecting
                .pop_front()
                .expect("More data received than expected");
            assert_eq!(v, exp);
            if expecting.is_empty() {
                okay.set(true);
            }
        }),
    );

    // First run passes single normal value
    fwd!([fwd], Some(123));
    s.run(s.now(), false);

    // Drop the Fwd to simulate actor failure
    drop(fwd);
    s.run(s.now(), false);
}
