//! The work the app is doing that takes long enough to be worth a list.
//!
//! A load used to be invisible: the page showed a skeleton and then it did not, and a load that
//! failed showed whatever the page shows when it has nothing. A registry makes the work itself
//! visible — what is in flight, what finished, what failed — which is what Yazi's task layer and
//! its `:tasks` list are for.
//!
//! What registers here is work the *app* asked for and can therefore name: the content load behind
//! a navigation request. Finishing is reported by the event that ends it, since the loads are
//! serialised — the navigation asks for one thing at a time — so the arrival of content finishes
//! whatever was in flight.

use std::time::Instant;

/// Where a task got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Done,
    Failed,
}

impl TaskState {
    pub fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// One piece of work.
#[derive(Debug, Clone)]
pub struct Task {
    /// A stable id: an index would point at a different task once the list is trimmed.
    id: u64,
    pub label: String,
    pub state: TaskState,
    pub started: Instant,
}

/// A handle to a task, so the event that ends it can say which one it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskId(u64);

/// What the app is doing, oldest first, capped.
#[derive(Debug, Default)]
pub struct Tasks {
    entries: Vec<Task>,
    /// The id the next task gets. Ids are never reused, so a handle to a task that has been
    /// dropped does not come to mean a different one.
    next_id: u64,
}

/// How many tasks are kept for `:tasks`.
const KEEP: usize = 32;

impl Tasks {
    /// Register a task and hand back its handle.
    pub fn begin(&mut self, label: impl Into<String>) -> TaskId {
        self.next_id += 1;
        self.entries.push(Task {
            id: self.next_id,
            label: label.into(),
            state: TaskState::Running,
            started: Instant::now(),
        });

        while self.entries.len() > KEEP {
            self.entries.remove(0);
        }

        TaskId(self.next_id)
    }

    /// Mark one task finished. An id that has been dropped (the list was full) is ignored: the
    /// task is gone, and there is nothing to say about it.
    pub fn finish(&mut self, id: TaskId, state: TaskState) {
        if let Some(task) = self.entries.iter_mut().find(|task| task.id == id.0) {
            task.state = state;
        }
    }

    /// Finish every task that is still running: what an event that ends "whatever was in flight"
    /// says. The id of the request is not carried by the event, and does not have to be — the
    /// navigation asks for one thing at a time.
    pub fn finish_running(&mut self, state: TaskState) {
        for task in self.entries.iter_mut().filter(|task| task.state.is_running()) {
            task.state = state;
        }
    }

    /// How many tasks are still running.
    pub fn running(&self) -> usize {
        self.entries
            .iter()
            .filter(|task| task.state.is_running())
            .count()
    }

    /// Every task, oldest first.
    pub fn all(&self) -> impl DoubleEndedIterator<Item = &Task> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget the tasks that are over: what a reader who has read the list wants next.
    pub fn clear_finished(&mut self) {
        self.entries.retain(|task| task.state.is_running());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task starts running, ends when it is told to, and only the running ones count as work.
    #[test]
    fn a_task_runs_until_it_is_finished() {
        let mut tasks = Tasks::default();
        let first = tasks.begin("加载 每日推荐");
        assert_eq!(tasks.running(), 1);

        let second = tasks.begin("加载 我喜欢的音乐");
        assert_eq!(tasks.running(), 2);

        tasks.finish(first, TaskState::Done);
        assert_eq!(tasks.running(), 1);
        assert_eq!(
            tasks.all().next().map(|task| task.state),
            Some(TaskState::Done)
        );

        tasks.finish(second, TaskState::Failed);
        assert_eq!(tasks.running(), 0);
    }

    /// An event that ends "whatever was in flight" ends every running task, and leaves the finished
    /// ones as they were: the list is a history, not just a progress readout.
    #[test]
    fn finishing_what_is_in_flight_leaves_the_history_alone() {
        let mut tasks = Tasks::default();
        let first = tasks.begin("first");
        tasks.finish(first, TaskState::Failed);
        tasks.begin("second");
        tasks.begin("third");

        tasks.finish_running(TaskState::Done);

        let states: Vec<TaskState> = tasks.all().map(|task| task.state).collect();
        assert_eq!(
            states,
            vec![TaskState::Failed, TaskState::Done, TaskState::Done],
            "the failure is still a failure"
        );
    }

    /// The list does not grow without bound, and a handle to a task that has been dropped is not a
    /// reason to panic when its event arrives.
    #[test]
    fn the_list_is_capped_and_a_dropped_task_is_ignored() {
        let mut tasks = Tasks::default();
        let oldest = tasks.begin("oldest");
        for i in 0..KEEP {
            tasks.begin(format!("task {i}"));
        }

        assert_eq!(tasks.all().count(), KEEP);
        assert_eq!(
            tasks.all().next().map(|task| task.label.as_str()),
            Some("task 0"),
            "the oldest was dropped"
        );

        // The handle is to a task that has been dropped: finishing it must not finish the task
        // that took its place in the list.
        tasks.finish(oldest, TaskState::Done);
        assert_eq!(
            tasks.running(),
            KEEP,
            "a dropped task's handle does not name another task"
        );

        tasks.clear_finished();
        assert_eq!(tasks.running(), KEEP, "nothing that is running was cleared");
    }
}
