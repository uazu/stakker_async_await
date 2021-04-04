use stakker::{ret, task::Task, task::TaskTrait, Core, Ret};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// Spawn a `Future` and pass the final result to the given `Ret`
///
/// This call does not support the `std::task::Waker` mechanism which
/// might be required by some futures.  Use [`spawn_with_waker`] if
/// you need that.  The final returned value is sent to `ret` as
/// `Some(value)`.  However if the the last reference to the task is
/// lost (e.g. an actor fails that is required to provide data for the
/// future), it sends `None`.
///
/// This call follows the **Stakker** convention of using absolutely
/// no synchronization: i.e. no mutexes, no atomics, etc.  Since
/// `std::task::Waker` is `Send` and so requires atomics or mutexes
/// and cross-thread signalling to operate, that cannot be used for
/// wake-ups.  An alternative Stakker-specific same-thread
/// synchronization-free mechanism is used instead by the trait
/// interfaces defined in this crate.  So long as the future only ever
/// blocks on the trait interfaces defined in this crate, everything
/// will run fine.  If it blocks on other things then you will get a
/// panic and need to use [`spawn_with_waker`] instead.
///
/// This call sets up the task and execution is deferred to the queue.
/// The task will then automatically execute in segments (from yield
/// to yield) as data it requires becomes available, until it
/// completes or is dropped.  Normally before a future yields, the
/// handling code takes a reference to the task using [`current_task`]
/// and stores it in a [`stakker::Ret`] or [`stakker::Fwd`] which
/// resumes the task once data becomes ready.
///
/// [`current_task`]: fn.current_task.html
/// [`spawn_with_waker`]: fn.spawn_with_waker.html
/// [`stakker::Fwd`]: ../stakker/struct.Fwd.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn spawn<T>(core: &mut Core, future: impl Future<Output = T> + 'static, ret: Ret<T>) {
    let mut task = Task::new(
        core,
        SpawnTask {
            future: Some(future),
            ret: Some(ret),
        },
    );

    core.defer(move |s| task.resume(s));
}

struct SpawnTask<T: 'static, F: Future<Output = T> + 'static> {
    // It would be more consistent if they were in the same Option,
    // but pinning complicates everything.  Since `ret` is consumed
    // when called, that means taking it out of the `Option`, but
    // since the future is pinned that must not be moved.
    future: Option<F>,   // Treat this as structural for pinning
    ret: Option<Ret<T>>, // Treat this as not structural for pinning
}

impl<T: 'static, F: Future<Output = T>> TaskTrait for SpawnTask<T, F> {
    fn resume(mut self: Pin<&mut Self>, _core: &mut Core) {
        // Projection: Safe because everything is pinned
        let opt_future = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.future) };
        if let Some(future) = opt_future.as_pin_mut() {
            let waker = unsafe { Waker::from_raw(panic_rawwaker()) };
            let mut cx = Context::from_waker(&waker);
            if let Poll::Ready(v) = future.poll(&mut cx) {
                // Projection: Safe because everything is pinned
                let mut opt_future = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.future) };
                opt_future.set(None);
                // Projection: Safe because we're always treating `ret` as non-structural
                let opt_ret = unsafe { &mut self.as_mut().get_unchecked_mut().ret };
                if let Some(ret) = opt_ret.take() {
                    ret!([ret], v);
                }
            }
        }
    }
}

// Standard library `std::task::Waker` is unfortunately `Send`, and we
// can't guarantee that someone won't attempt to move a waker to
// another thread (e.g. the timer example from the docs).  So the
// solution is that we just don't use the waker mechanism internally.
fn panic_rawwaker() -> RawWaker {
    RawWaker::new(&DUMMY, &PANIC_VTABLE)
}

const PANIC_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| rawwaker_panic(), // clone
    |_| rawwaker_panic(), // wake
    |_| rawwaker_panic(), // wake_by_ref
    |_| {},               // drop (ignore)
);

const DUMMY: () = ();

fn rawwaker_panic() -> ! {
    panic!("Attempted to wake/clone a Stakker task that was spawned using `spawn()`.  Try `spawn_with_waker()` instead.");
}

/// Spawn a `Future` and pass the final result to the given `Ret`,
/// with `Waker` support
///
/// NOT YET IMPLEMENTED!
///
/// This call supports the `std::task::Waker` mechanism.  This has an
/// additional cost because things have to be set up to receive those
/// inter-thread notifications.  It also requires that a **Stakker**
/// inter-thread waker mechanism has been set up via
/// [`stakker::Stakker::set_poll_waker`] by the event-polling
/// implementation.  For example the `stakker_mio` crate supports
/// this.
///
/// [`stakker::Stakker::set_poll_waker`]: ../stakker/struct.Stakker.html#method.set_poll_waker
pub fn spawn_with_waker<T>(
    _core: &mut Core,
    _future: impl Future<Output = T> + 'static,
    _ret: Ret<T>,
) {
    todo!(); // TODO
}
