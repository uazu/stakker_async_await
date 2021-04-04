//! Support for Rust standard library traits

use crate::{current_task, Task};
use stakker::{ret, ret_do, ret_some_do, Core, Deferrer, Ret};
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::rc::Rc;
use std::rc::Weak;
use std::task::{Context, Poll};

// TODO: Replace all RefCell with ActorCell to statically guarantee
// correctness of borrowing.  But this means exposing some types and
// methods from Stakker crate.

/// Create a push-style `Ret` &rarr; `Future` pipe, that passes
/// through failure
///
/// This returns a future which will resolve to a value only once a
/// value is provided to the returned [`stakker::Ret`] instance, or if
/// the [`stakker::Ret`] is dropped.  If the [`stakker::Ret`] instance
/// is dropped (for example if the actor holding it fails), then
/// `None` is passed, otherwise `Some(v)`.  If you want the whole task
/// to terminate abruptly if the [`stakker::Ret`] is dropped and that
/// value is awaited, then see [`future_push_some`].
///
/// [`future_push_some`]: fn.future_push_some.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn future_push<T>(core: &mut Core) -> (impl Future<Output = Option<T>>, Ret<T>) {
    let aux = Rc::new(RefCell::new(FuturePushAux {
        value: None,
        task: None,
        deferrer: core.deferrer(),
    }));

    let deferrer = core.deferrer();
    let fut = FuturePush {
        aux: Rc::downgrade(&aux),
    };
    let ret = ret_do!(move |m| {
        // Defer it to guarantee that the RefCell borrow won't fail
        deferrer.defer(move |s| {
            let mut aux = aux.borrow_mut();
            aux.value = Some(m);
            if let Some(mut task) = aux.task.take() {
                drop(aux);
                task.resume(s);
            }
        });
    });

    (fut, ret)
}

/// Create a push-style `Ret` &rarr; `Future` pipe, that drops on
/// failure
///
/// This returns a future which will resolve to a value only once a
/// value is provided to the returned [`stakker::Ret`] instance.  If
/// the [`stakker::Ret`] instance is dropped (for example if the actor
/// holding it fails), then if that future is awaited then the task
/// will also be dropped, which means that the task will be stopped
/// abruptly at the current `await`.  To handle [`stakker::Ret`] being
/// dropped, see [`future_push`].
///
/// [`future_push`]: fn.future_push.html
/// [`stakker::Ret`]: ../stakker/struct.Ret.html
pub fn future_push_some<T>(core: &mut Core) -> (impl Future<Output = T>, Ret<T>) {
    let aux = Rc::new(RefCell::new(FuturePushAux {
        value: None,
        task: None,
        deferrer: core.deferrer(),
    }));

    let deferrer = core.deferrer();
    let fut = FuturePush {
        aux: Rc::downgrade(&aux),
    };
    let ret = ret_some_do!(move |m| {
        // Defer it to guarantee that the RefCell borrow won't fail
        deferrer.defer(move |s| {
            let mut aux = aux.borrow_mut();
            aux.value = Some(m);
            if let Some(mut task) = aux.task.take() {
                drop(aux);
                task.resume(s);
            }
        });
    });

    (fut, ret)
}

struct FuturePushAux<T> {
    // Either `value` or `task` may arrive first
    value: Option<T>,
    task: Option<Task>,
    deferrer: Deferrer,
}

/// Used to implement `future_push` and `future_push_some`
pub struct FuturePush<T> {
    aux: Weak<RefCell<FuturePushAux<T>>>,
}

impl<T> Future for FuturePush<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        if let Some(aux) = self.aux.upgrade() {
            let mut aux = aux.borrow_mut();
            match aux.value.take() {
                Some(v) => Poll::Ready(v),
                None => {
                    aux.task = current_task(&mut aux.deferrer);
                    Poll::Pending
                }
            }
        } else {
            // The Ret has gone away, so this can never return a
            // value.  So the expected behaviour is that the last Task
            // reference has been dropped, so when this returns the
            // whole Future will also be dropped.
            Poll::Pending
        }
    }
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
pub fn future_pull<T>(core: &mut Core, ret: Ret<Ret<T>>) -> impl Future<Output = Option<T>> {
    let aux = Rc::new(RefCell::new(FuturePullAux::Done));
    let deferrer = core.deferrer();
    let aux2 = aux.clone();
    let ret2 = ret_do!(move |m| {
        // Defer it to guarantee that the RefCell borrow won't fail
        deferrer.defer(move |s| {
            let mut aux = aux2.borrow_mut();
            if let FuturePullAux::Polled(mut task) =
                mem::replace(&mut *aux, FuturePullAux::Value(m))
            {
                drop(aux);
                task.resume(s);
                return;
            }
            // Intentionally drop the Task to get it to die
            *aux = FuturePullAux::Done;
        })
    });

    *aux.borrow_mut() = FuturePullAux::Waiting(ret, ret2, core.deferrer());
    FuturePull { aux }
}

/// Create a pull-style `Ret` &rarr; `Future` pipe, that drops on
/// failure
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
pub fn future_pull_some<T>(core: &mut Core, ret: Ret<Ret<T>>) -> impl Future<Output = T> {
    let aux = Rc::new(RefCell::new(FuturePullAux::Done));
    let deferrer = core.deferrer();
    let aux2 = aux.clone();
    let ret2 = ret_do!(move |m| {
        // Defer it to guarantee that the RefCell borrow won't fail
        deferrer.defer(move |s| {
            let mut aux = aux2.borrow_mut();
            if let Some(m) = m {
                if let FuturePullAux::Polled(mut task) =
                    mem::replace(&mut *aux, FuturePullAux::Value(m))
                {
                    drop(aux);
                    task.resume(s);
                    return;
                }
            }
            // Intentionally drop the Task to get it to die
            *aux = FuturePullAux::Done;
        })
    });

    *aux.borrow_mut() = FuturePullAux::Waiting(ret, ret2, core.deferrer());
    FuturePull { aux }
}

enum FuturePullAux<T: 'static, V: 'static> {
    Waiting(Ret<Ret<T>>, Ret<T>, Deferrer),
    Polled(Task),
    Value(V),
    Done,
}

/// Used to implement `future_pull` and `future_pull_some`
pub struct FuturePull<T: 'static, V: 'static> {
    aux: Rc<RefCell<FuturePullAux<T, V>>>,
}

impl<T, V> Future for FuturePull<T, V> {
    type Output = V;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<V> {
        let mut aux = self.aux.borrow_mut();
        match mem::replace(&mut *aux, FuturePullAux::Done) {
            FuturePullAux::Waiting(ret, ret2, mut deferrer) => {
                if let Some(task) = current_task(&mut deferrer) {
                    ret!([ret], ret2);
                    *aux = FuturePullAux::Polled(task);
                }
                Poll::Pending
            }
            FuturePullAux::Polled(w) => {
                *aux = FuturePullAux::Polled(w);
                Poll::Pending
            }
            FuturePullAux::Value(v) => Poll::Ready(v),
            FuturePullAux::Done => Poll::Pending,
        }
    }
}
