use std::{sync::{Arc, RwLock}, collections::VecDeque};

use crate::state::state::State;

#[derive(Debug, PartialEq)]
pub enum ExecutionQueueStatus
{
    Waiting,
    Running,
    Done
}

pub type ExecutionQueueResult = Arc<RwLock<ExecutionQueueStatus>>;
pub type ExecutionQueueItem = Arc<RwLock<ExecutionQueue>>;

pub struct ExecutionItem
{
    func: Box<dyn Fn(&mut State) + Send + Sync>,
    status: Arc<RwLock<ExecutionQueueStatus>>
}

impl ExecutionItem
{
    pub fn new(func: Box<dyn Fn(&mut State) + Send + Sync>) -> ExecutionItem
    {
        ExecutionItem
        {
            func: func,
            status: Arc::new(RwLock::new(ExecutionQueueStatus::Waiting))
        }
    }
}

pub struct ExecutionQueue
{
    queue: VecDeque<ExecutionItem>
}

impl ExecutionQueue
{
    pub fn new() -> ExecutionQueue
    {
        ExecutionQueue
        {
            queue: VecDeque::new()
        }
    }

    pub fn add(&mut self, func: Box<dyn Fn(&mut State) + Send + Sync>) -> ExecutionQueueResult
    {
        let item = ExecutionItem::new(func);
        let result = item.status.clone();

        self.queue.push_back(item);

        result
    }

    pub fn run_first(&mut self, state: &mut State)
    {
        if let Some(item) = self.queue.pop_front()
        {
            {
                *item.status.write().unwrap() = ExecutionQueueStatus::Running;
            }

            {
                (item.func)(state);
            }

            {
                *item.status.write().unwrap() = ExecutionQueueStatus::Done;
            }
        }
    }

    pub fn run_all(&mut self, state: &mut State)
    {
        while self.queue.len() > 0
        {
            self.run_first(state);
        }
    }
}