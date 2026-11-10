#![allow(dead_code)]

use std::{sync::{Arc, RwLock}, collections::VecDeque};

use crate::state::state::State;

use super::thread::{is_ended_run_session, run_session, sleep_millis};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ExecutionQueueStatus
{
    Waiting,
    Running,
    Done
}

//pub type ExecutionQueueResult = Arc<RwLock<ExecutionQueueStatus>>;
pub type ExecutionQueueItem = Arc<RwLock<ExecutionQueue>>;

pub struct ExecutionQueueResult
{
    status: Arc<RwLock<ExecutionQueueStatus>>
}

impl ExecutionQueueResult
{
    pub fn new(status: Arc<RwLock<ExecutionQueueStatus>>) -> ExecutionQueueResult
    {
        ExecutionQueueResult
        {
            status
        }
    }

    pub fn state(&self) -> ExecutionQueueStatus
    {
        *self.status.read().unwrap()
    }

    pub fn join(&self)
    {
        while *self.status.read().unwrap() != ExecutionQueueStatus::Done
        {
            sleep_millis(1);
        }
    }
}

pub struct ExecutionItem
{
    func: Box<dyn Fn(&mut State) + Send + Sync>,
    status: Arc<RwLock<ExecutionQueueStatus>>,
    run_session: u64,
}

impl ExecutionItem
{
    pub fn new(func: Box<dyn Fn(&mut State) + Send + Sync>) -> ExecutionItem
    {
        ExecutionItem
        {
            func: func,
            status: Arc::new(RwLock::new(ExecutionQueueStatus::Waiting)),
            run_session: run_session(),
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

        ExecutionQueueResult::new(result)
    }

    pub fn run_first(queue: Arc<RwLock<ExecutionQueue>>, state: &mut State)
    {
        let front ;
        {
            front = queue.write().unwrap().queue.pop_front();
        }

        if let Some(item) = front
        {
            {
                *item.status.write().unwrap() = ExecutionQueueStatus::Running;
            }

            // skips items of an ended run - the waiting thread continues
            if !is_ended_run_session(item.run_session)
            {
                (item.func)(state);
            }

            {
                *item.status.write().unwrap() = ExecutionQueueStatus::Done;
            }
        }
    }

    pub fn run_all(queue: Arc<RwLock<ExecutionQueue>>, state: &mut State)
    {
        while queue.read().unwrap().queue.len() > 0
        {
            Self::run_first(queue.clone(), state);
        }
    }
}