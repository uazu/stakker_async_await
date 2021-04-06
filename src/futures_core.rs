use crate::{spawn_future, spawn_future_with_waker, ActorFail};
use futures_core::Stream;
use stakker::task::Task;
use stakker::{fwd, fwd_do, ret_nop, Core, Deferrer, Fwd};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

/// Spawn a `Stream`, forwarding values to a `Fwd`
///
/// This call does not support the `std::task::Waker` mechanism.  See
/// [`spawn`] for details.
///
/// [`spawn`]: fn.spawn.html
pub fn spawn_stream<T>(
    core: &mut Core,
    stream: impl Stream<Item = T> + 'static,
    fwd: Fwd<Option<T>>,
) {
    spawn_future(
        core,
        StreamToFwd {
            stream: Box::pin(stream),
            fwd,
        },
        ret_nop!(),
    );
}

/// Spawn a `Stream`, forwarding values to a `Fwd`, with `Waker` support.
///
/// This call does not support the `std::task::Waker` mechanism.  See
/// [`spawn_with_waker`] for details.
///
/// [`spawn_with_waker`]: fn.spawn_with_waker.html
pub fn spawn_stream_with_waker<T>(
    core: &mut Core,
    stream: impl Stream<Item = T> + 'static,
    fwd: Fwd<Option<T>>,
) {
    spawn_future_with_waker(
        core,
        StreamToFwd {
            stream: Box::pin(stream),
            fwd,
        },
        ret_nop!(),
    );
}

pub struct StreamToFwd<T: 'static> {
    stream: Pin<Box<dyn Stream<Item = T> + 'static>>,
    fwd: Fwd<Option<T>>,
}

impl<T> Future for StreamToFwd<T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            return match self.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(v)) => {
                    fwd!([self.fwd], Some(v));
                    continue;
                }
                Poll::Ready(None) => {
                    fwd!([self.fwd], None);
                    Poll::Ready(())
                }
                Poll::Pending => Poll::Pending,
            };
        }
    }
}

#[macro_use]
macro_rules! resume {
    ($deferrer:expr, $task:ident) => {
        $deferrer.defer(move |s| $task.resume(s))
    };
}

/// Create a `Fwd` &rarr; `Stream` pipe which drops the task on failure
///
/// The values sent to the returned `Fwd` should be any number of
/// `Some(value)`, then finally a `None` to terminate the stream.  If
/// the stream is not terminated with a `None` before the `Fwd` is
/// dropped then that counts as irregular termination and the
/// async/await task will be dropped.  This is because this is what
/// would happen if the actor unexpectedly failed.  So in normal
/// operation you must make sure that the stream is terminated.
///
/// There are three ways to operate this pipe:
///
/// - Send all the data immediately to the returned `Fwd`.  It will be
/// queued until the stream owner is ready to handle it.  `more` can
/// be passed as `fwd_nop!()` as it is not required.
///
/// - For each message received on `more`, send a batch of items
/// through the returned `Fwd`.  When the available data is finished,
/// send a `None`.  This lets both ends operate efficiently.
///
/// - For each message received on `more`, send a single item through
/// the returned `Fwd`.  If there is no more data, send a `None`.
/// This means that data is only sent when the async/await task
/// requests it.
///
/// The `init_capacity` is the initial capacity of the queue.  If you
/// know that you're going to be sending through batches of a certain
/// size, then pass that size here.  Otherwise the queue will expand
/// as necessary, so any sensible value will be okay.
pub fn fwd_to_stream<T>(
    core: &mut Core,
    more: Fwd<()>,
    init_capacity: usize,
) -> (impl Stream<Item = T>, Fwd<Option<T>>) {
    let spq = Rc::new(RefCell::new(StreamQueue::new(
        core.deferrer(),
        more,
        init_capacity,
    )));

    let stream = StreamPipe { spq: spq.clone() };
    let guard = StreamGuardDrop(spq);
    let fwd = fwd_do!(move |m| {
        let mut spq = guard.0.borrow_mut();
        if !spq.end {
            if let Some(m) = m {
                spq.queue.push_back(m);
            } else {
                spq.end = true;
            }
            if let Some(mut task) = spq.task.take() {
                resume!(spq.deferrer, task);
            }
        }
    });

    (stream, fwd)
}

/// Create a `Fwd` &rarr; `Stream` pipe which passes through failure
///
/// The values sent to the returned `Fwd` should be any number of
/// `Some(value)`, then finally a `None` to terminate the stream.
/// These values come out of the stream as `Some(Ok(value))` and
/// `None`.  If the stream is not terminated with a `None` before the
/// `Fwd` is dropped then that counts as irregular termination and the
/// values `Some(Err(ActorFail))` and `None` will come out of the
/// stream to allow it to handle the failure.  This usually indicates
/// that an actor unexpectedly failed.  So in normal operation you
/// must make sure that the stream is terminated.
///
/// There are three ways to operate this pipe:
///
/// - Send all the data immediately to the returned `Fwd`.  It will be
/// queued until the stream owner is ready to handle it.  `more` can
/// be passed as `fwd_nop!()` as it is not required.
///
/// - For each message received on `more`, send a batch of items
/// through the returned `Fwd`.  When the available data is finished,
/// send a `None`.  This lets both ends operate efficiently.
///
/// - For each message received on `more`, send a single item through
/// the returned `Fwd`.  If there is no more data, send a `None`.
/// This means that data is only sent when the async/await task
/// requests it.
///
/// The `init_capacity` is the initial capacity of the queue.  If you
/// know that you're going to be sending through batches of a certain
/// size, then pass that size here.  Otherwise the queue will expand
/// as necessary, so any sensible value will be okay.
pub fn fwd_to_stream_result<T>(
    core: &mut Core,
    more: Fwd<()>,
    init_capacity: usize,
) -> (impl Stream<Item = Result<T, ActorFail>>, Fwd<Option<T>>) {
    let spq = Rc::new(RefCell::new(StreamQueue::new(
        core.deferrer(),
        more,
        init_capacity,
    )));

    let stream = StreamPipe { spq: spq.clone() };
    let guard = StreamGuardFail(spq);
    let fwd = fwd_do!(move |m| {
        let mut spq = guard.0.borrow_mut();
        if !spq.end {
            if let Some(m) = m {
                spq.queue.push_back(Ok(m));
            } else {
                spq.end = true;
            }
            if let Some(mut task) = spq.task.take() {
                resume!(spq.deferrer, task);
            }
        }
    });

    (stream, fwd)
}

struct StreamQueue<T> {
    deferrer: Deferrer,
    more: Fwd<()>,
    queue: VecDeque<T>,
    end: bool,
    drop: bool,
    task: Option<Task>,
}

impl<T> StreamQueue<T> {
    fn new(deferrer: Deferrer, more: Fwd<()>, init_capacity: usize) -> Self {
        Self {
            deferrer,
            more,
            queue: VecDeque::with_capacity(init_capacity),
            end: false,
            drop: false,
            task: None,
        }
    }
}

// Guard that when dropped, drops the task if the stream was not
// terminated nicely
struct StreamGuardDrop<T>(Rc<RefCell<StreamQueue<T>>>);

impl<T> Drop for StreamGuardDrop<T> {
    fn drop(&mut self) {
        let mut spq = self.0.borrow_mut();
        if !spq.end {
            // If not ended nicely, drop the task
            spq.drop = true;
            spq.task = None;
        }
    }
}

// Guard that when dropped, adds an error to the stream if it was not
// terminated nicely
struct StreamGuardFail<T>(Rc<RefCell<StreamQueue<Result<T, ActorFail>>>>);

impl<T> Drop for StreamGuardFail<T> {
    fn drop(&mut self) {
        let mut spq = self.0.borrow_mut();
        if !spq.end {
            // If not ended nicely, append an error
            spq.queue.push_back(Err(ActorFail));
            spq.end = true;
            if let Some(mut task) = spq.task.take() {
                resume!(spq.deferrer, task);
            }
        }
    }
}

/// `Stream` used to implement `stream` and `stream_result`
pub struct StreamPipe<T> {
    spq: Rc<RefCell<StreamQueue<T>>>,
}

impl<T> Stream for StreamPipe<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<T>> {
        let mut spq = self.spq.borrow_mut();
        if spq.drop {
            spq.task = None;
            return Poll::Pending;
        }
        if let Some(item) = spq.queue.pop_front() {
            return Poll::Ready(Some(item));
        }
        if spq.end {
            return Poll::Ready(None);
        }
        spq.task = crate::current_task(&mut spq.deferrer);

        // Notify sender that we need some more data
        let more = spq.more.clone();
        drop(spq);
        fwd!([more]);

        Poll::Pending
    }
}
