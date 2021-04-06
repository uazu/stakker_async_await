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

mod futures_core;
mod rust_std;
mod spawn;

pub use crate::futures_core::{
    fwd_to_stream, fwd_to_stream_result, spawn_stream, spawn_stream_with_waker,
};
pub use crate::rust_std::{future_pull, future_pull_result, ret_to_future, ret_to_future_result};
pub use spawn::{spawn_future, spawn_future_with_waker};
use stakker::task::Task;

/// Get a reference to the current task
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

/// Error representing failure of an actor
#[derive(Debug, PartialEq, Eq)]
pub struct ActorFail;

impl std::error::Error for ActorFail {}

impl std::fmt::Display for ActorFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Actor failure")
    }
}
