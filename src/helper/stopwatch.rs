#![allow(dead_code)]

use web_time::{Instant, Duration};

pub struct StopWatch
{
    start_time: Option<Instant>,
    elapsed: Duration,
}

impl StopWatch
{
    pub fn new(start: bool) -> Self
    {
        let mut sw = StopWatch
        {
            start_time: None,
            elapsed: Duration::new(0, 0),
        };

        if start
        {
            sw.start();
        }
        sw
    }

    pub fn start(&mut self)
    {
        if self.start_time.is_none()
        {
            self.start_time = Some(Instant::now());
        }
    }

    pub fn stop(&mut self)
    {
        if let Some(start) = self.start_time
        {
            self.elapsed += start.elapsed();
            self.start_time = None;
        }
    }

    pub fn reset(&mut self)
    {
        self.start_time = None;
        self.elapsed = Duration::new(0, 0);
    }

    pub fn resume(&mut self)
    {
        if self.start_time.is_none()
        {
            self.start_time = Some(Instant::now());
        }
    }

    pub fn set_time(&mut self, duration: Duration)
    {
        self.elapsed = duration;
        self.start_time = Some(Instant::now() - duration);
    }

    pub fn append_time(&mut self, duration: Duration)
    {
        self.elapsed += duration;
    }

    pub fn get_time(&self) -> Duration
    {
        if let Some(start) = self.start_time
        {
            return self.elapsed + start.elapsed();
        }
        self.elapsed
    }

    pub fn get_time_ms(&self) -> u128
    {
        self.get_time().as_millis()
    }

    pub fn is_running(&self) -> bool
    {
        self.start_time.is_some()
    }
}
