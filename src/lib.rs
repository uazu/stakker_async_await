//! **Stakker** interface to Rust async/await
//!
//! **Stakker** has two primitives for asynchronous callbacks,
//! i.e. for passing data from one point to another:
//!
//! - [`stakker::Ret`] accepts a single value to pass to an end-point,
//! and is consumed when used to guarantee that it is not used again
//!
//! - [`stakker::Fwd`] accepts an endless stream of values to pass to
//! an end-point, and can be cloned
//!
//! An end-point would typically be a method on an actor, and the
//! value would be passed in a call to that method.  However other
//! types of end-point are possible.
//!
//! The Rust standard library `Future` is like a `Ret` in reverse.  A
//! `Ret` sends the response to an end-point, but a `Future` is held
//! by the end-point to receive the response.
//!
//! [`stakker::Fwd`]: ../stakker/struct.Fwd.html
//! [`stakker::Ret`]: ../stakker/struct.Ret.html

use stakker::Deferrer;

mod rust;
mod spawn;

pub use rust::{future_pull, future_pull_some, future_push, future_push_some};
pub use spawn::{spawn, spawn_with_waker};
use stakker::task::Task;

/// Get a reference to the current `Task`
///
/// Returns a reference to the task if we're running within one, or
/// else returns `None`.  The returned [`stakker::task::Task`] can be
/// saved and used to resume the execution of the future when new data
/// becomes available.
///
/// [`stakker::task::Task`]: ../stakker/task/struct.Task.html
pub fn current_task(deferrer: &mut Deferrer) -> Option<Task> {
    Task::from_context(deferrer)
}
