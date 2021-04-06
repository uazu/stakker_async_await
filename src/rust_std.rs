//! Support for Rust standard library traits

use crate::{current_task, ActorFail, Task};
use stakker::{ret, ret_do, Core, Deferrer, Ret};
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

#[macro_use]
macro_rules! resume {
    ($deferrer:expr, $task:ident) => {
        $deferrer.defer(move |s| $task.resume(s))
    };
}

/// Create a push-style `Ret` &rarr; `Future` pipe, that drops the
/// task on failure
///
/// This returns a future which will resolve to a value only once a
/// value is provided to the returned [`stakker::Ret`] instance.  If
/// the [`stakker::Ret`] instance is dropped (for example if the actor
/// holding it fails), then if that future is awaited then the task
/// will also be dropped, which means that the task will be stopped
/// abruptly at the current `await`.  To handle [`stakker::Ret`] being
/// dropped, see [`ret_to_future_result`].
///
/// [`ret_to_future_result`]: fn.ret_to_future_result.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn ret_to_future<T>(core: &mut Core) -> (impl Future<Output = T>, Ret<T>) {
    let aux = Rc::new(RefCell::new(FuturePushAux {
        value: None,
        task: None,
        deferrer: core.deferrer(),
    }));

    let fut = FuturePush { aux: aux.clone() };
    let ret = ret_do!(move |m| {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = aux.borrow_mut();
        aux.value = Some(m); // Some(Some) or Some(None)
        if let Some(mut task) = aux.task.take() {
            resume!(aux.deferrer, task);
        }
    });

    (fut, ret)
}

/// Create a push-style `Ret` &rarr; `Future` pipe, that passes
/// through failure
///
/// This returns a future which will resolve to a value only once a
/// value is provided to the returned [`stakker::Ret`] instance, or if
/// the [`stakker::Ret`] is dropped.  If the [`stakker::Ret`] instance
/// is dropped (for example if the actor holding it fails), then
/// `Err(ActorFail)` is passed, otherwise `Ok(v)`.  If you want the
/// whole task to terminate abruptly if the [`stakker::Ret`] is
/// dropped and that value is awaited, then see [`ret_to_future`].
///
/// [`ret_to_future`]: fn.ret_to_future.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn ret_to_future_result<T>(
    core: &mut Core,
) -> (impl Future<Output = Result<T, ActorFail>>, Ret<T>) {
    let aux = Rc::new(RefCell::new(FuturePushAux {
        value: None,
        task: None,
        deferrer: core.deferrer(),
    }));

    let fut = FuturePush { aux: aux.clone() };
    let ret = ret_do!(move |m| {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = aux.borrow_mut();
        aux.value = Some(Some(m.ok_or(ActorFail)));
        if let Some(mut task) = aux.task.take() {
            resume!(aux.deferrer, task);
        }
    });

    (fut, ret)
}

// Either `value` or `task` may arrive first
struct FuturePushAux<T> {
    // None: not ready, Some(None): drop task, Some(Some(v)): pass result
    value: Option<Option<T>>,
    task: Option<Task>,
    deferrer: Deferrer,
}

/// Used to implement `future_push` and `future_push_some`
pub struct FuturePush<T> {
    aux: Rc<RefCell<FuturePushAux<T>>>,
}

impl<T> Future for FuturePush<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = self.aux.borrow_mut();
        match aux.value.take() {
            Some(Some(v)) => Poll::Ready(v),
            Some(None) => Poll::Pending, // Current task will be dropped
            None => {
                aux.task = current_task(&mut aux.deferrer);
                Poll::Pending
            }
        }
    }
}

/// Create a pull-style `Ret` &rarr; `Future` pipe, that drops the
/// task on failure
///
/// This returns a `Future` which when polled will send a message to
/// the provided [`stakker::Ret`] instance in order to request a
/// value.  So it "pulls" the value only on demand.  The provided
/// [`stakker::Ret`] instance is given a [`stakker::Ret`] to use to
/// send back the value.  If the value is not provided, either because
/// there is nothing left running to handle the [`stakker::Ret`] sent,
/// or if the provided [`stakker::Ret`] is dropped, then the reference
/// to the task is dropped, which means that the whole async execution
/// of that task will end abruptly at the current `.await`.  To handle
/// [`stakker::Ret`] being dropped, see [`future_pull`].
///
/// [`future_pull`]: fn.future_pull.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn future_pull<T>(core: &mut Core, ret: Ret<Ret<T>>) -> impl Future<Output = T> {
    let aux = Rc::new(RefCell::new(FuturePullAux::new(core.deferrer())));
    let aux2 = aux.clone();
    let ret2 = ret_do!(move |m| {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = aux2.borrow_mut();
        if let Some(m) = m {
            if let FPState::Polled(mut task) = mem::replace(&mut aux.state, FPState::Value(m)) {
                resume!(aux.deferrer, task);
                return;
            }
        }
        // Intentionally drop the Task to get it to die
        aux.state = FPState::Done;
    });

    aux.borrow_mut().state = FPState::Waiting(ret, ret2);
    FuturePull { aux }
}

/// Create a pull-style `Ret` &rarr; `Future` pipe, that passes
/// through failure
///
/// This returns a future which when polled will send a message to the
/// provided [`stakker::Ret`] instance in order to request a value.
/// So it "pulls" the value only on demand.  The provided
/// [`stakker::Ret`] instance is given a [`stakker::Ret`] to use to
/// send back the value.  If the value is not provided, either because
/// there is nothing left running to handle the [`stakker::Ret`] sent,
/// or if the provided [`stakker::Ret`] is dropped, then the future
/// resolves to `None`.  If a value is passed through then it resolves
/// to `Some(value)`.  To ignore `None` values and drop the task, see
/// [`future_pull_some`].
///
/// [`future_pull_some`]: fn.future_pull_some.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn future_pull_result<T>(
    core: &mut Core,
    ret: Ret<Ret<T>>,
) -> impl Future<Output = Result<T, ActorFail>> {
    let aux = Rc::new(RefCell::new(FuturePullAux::new(core.deferrer())));
    let aux2 = aux.clone();
    let ret2 = ret_do!(move |m| {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = aux2.borrow_mut();
        if let FPState::Polled(mut task) =
            mem::replace(&mut aux.state, FPState::Value(m.ok_or(ActorFail)))
        {
            resume!(aux.deferrer, task);
        } else {
            // Intentionally drop the Task to get it to die
            aux.state = FPState::Done;
        }
    });

    aux.borrow_mut().state = FPState::Waiting(ret, ret2);
    FuturePull { aux }
}

struct FuturePullAux<T: 'static, V: 'static> {
    state: FPState<T, V>,
    deferrer: Deferrer,
}

impl<T: 'static, V: 'static> FuturePullAux<T, V> {
    fn new(deferrer: Deferrer) -> Self {
        Self {
            state: FPState::Done,
            deferrer,
        }
    }
}

enum FPState<T: 'static, V: 'static> {
    Waiting(Ret<Ret<T>>, Ret<T>),
    Polled(Task),
    Value(V),
    Done,
}

/// Used to implement `future_pull` and `future_pull_result`
pub struct FuturePull<T: 'static, V: 'static> {
    aux: Rc<RefCell<FuturePullAux<T, V>>>,
}

impl<T, V> Future for FuturePull<T, V> {
    type Output = V;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<V> {
        // Borrow-safety: Safe because we don't call anything that
        // might borrow whilst we hold it, and other borrows do the same
        let mut aux = self.aux.borrow_mut();
        match mem::replace(&mut aux.state, FPState::Done) {
            FPState::Waiting(ret, ret2) => {
                if let Some(task) = current_task(&mut aux.deferrer) {
                    aux.state = FPState::Polled(task);
                    drop(aux);
                    ret!([ret], ret2);
                }
                Poll::Pending
            }
            FPState::Polled(task) => {
                aux.state = FPState::Polled(task);
                Poll::Pending
            }
            FPState::Value(v) => Poll::Ready(v),
            FPState::Done => Poll::Pending,
        }
    }
}
