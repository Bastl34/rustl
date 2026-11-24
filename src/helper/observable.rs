#![allow(dead_code)]

// Babylon.js-style observer system.
//
// An `Observable<T>` is stored as a field on the owning type `T`. Callbacks
// receive `&mut T` when fired. Because the observable is a field of `T`,
// notifying would normally be a self-borrow conflict — the `notify` helper
// solves this by temporarily moving the observer list out, invoking the
// callbacks, then merging the list back in (including any observers that
// were added during the notify, minus any that were removed or were one-shot).

use std::fmt;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};

// all render observers of nodes and instances (their on_before_render / on_after_render) - none: the scene skips the walk over every instance
pub static RENDER_OBSERVERS: AtomicUsize = AtomicUsize::new(0);

// the same for their on_before_update / on_after_update - none: the node update takes no write lock just to find nobody
pub static UPDATE_OBSERVERS: AtomicUsize = AtomicUsize::new(0);

pub fn anyone(tally: &AtomicUsize) -> bool
{
    tally.load(Ordering::Relaxed) > 0
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(u64);

impl fmt::Debug for ObserverId
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        write!(f, "ObserverId({})", self.0)
    }
}

pub struct Entry<T: ?Sized>
{
    id: ObserverId,
    once: bool,
    cb: Box<dyn FnMut(&mut T) + Send + Sync + 'static>,
}

pub struct Observable<T: ?Sized>
{
    observers: Vec<Entry<T>>,
    next_id: u64,
    notifying: bool,
    pending_remove: Vec<ObserverId>,
    tally: Option<&'static AtomicUsize>, // counts the observers of all observables of one kind (see RENDER_OBSERVERS)
}

impl<T: ?Sized> Drop for Observable<T>
{
    fn drop(&mut self)
    {
        self.count(-(self.observers.len() as isize));
    }
}

impl<T: ?Sized> Default for Observable<T>
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl<T: ?Sized> Observable<T>
{
    pub fn new() -> Self
    {
        Self
        {
            observers: Vec::new(),
            next_id: 0,
            notifying: false,
            pending_remove: Vec::new(),
            tally: None,
        }
    }

    // its observers also count in tally
    pub fn counted(tally: &'static AtomicUsize) -> Self
    {
        let mut observable = Self::new();
        observable.tally = Some(tally);
        observable
    }

    fn count(&self, delta: isize)
    {
        let Some(tally) = self.tally else { return; };
        if delta > 0 { tally.fetch_add(delta as usize, Ordering::Relaxed); }
        else if delta < 0 { tally.fetch_sub((-delta) as usize, Ordering::Relaxed); }
    }

    // the observer list after a notify: the drained ones that stay, then the ones added meanwhile, minus the removed ones
    fn merge_after_notify(&mut self, mut drained: Vec<Entry<T>>)
    {
        let before = drained.len() + self.observers.len();
        self.notifying = false;

        let removed = std::mem::take(&mut self.pending_remove);

        // drop one-shot entries that fired and entries removed during the notify
        drained.retain(|e| !e.once && !removed.contains(&e.id));

        // anything added mid-notify is currently in self.observers; preserve original
        // order by putting the drained (surviving) list first, then the newcomers
        let added_during = std::mem::take(&mut self.observers);
        self.observers = drained;
        self.observers.extend(added_during);

        // in case a callback also tried to remove something added mid-notify
        if !removed.is_empty()
        {
            self.observers.retain(|e| !removed.contains(&e.id));
        }

        self.count(self.observers.len() as isize - before as isize);
    }

    pub fn add(&mut self, cb: impl FnMut(&mut T) + Send + Sync + 'static) -> ObserverId
    {
        self.push_entry(false, Box::new(cb))
    }

    pub fn add_once(&mut self, cb: impl FnMut(&mut T) + Send + Sync + 'static) -> ObserverId
    {
        self.push_entry(true, Box::new(cb))
    }

    fn push_entry(&mut self, once: bool, cb: Box<dyn FnMut(&mut T) + Send + Sync + 'static>) -> ObserverId
    {
        self.next_id += 1;
        let id = ObserverId(self.next_id);
        self.observers.push(Entry { id, once, cb });
        self.count(1);
        id
    }

    pub fn remove(&mut self, id: ObserverId) -> bool
    {
        if self.notifying
        {
            // can't mutate the in-flight list directly; defer
            self.pending_remove.push(id);
            return true;
        }

        let len = self.observers.len();
        self.observers.retain(|e| e.id != id);
        self.count(self.observers.len() as isize - len as isize);
        self.observers.len() != len
    }

    pub fn clear(&mut self)
    {
        if self.notifying
        {
            for e in &self.observers
            {
                self.pending_remove.push(e.id);
            }
            return;
        }
        self.count(-(self.observers.len() as isize));
        self.observers.clear();
    }

    pub fn has_observers(&self) -> bool
    {
        !self.observers.is_empty()
    }

    pub fn len(&self) -> usize
    {
        self.observers.len()
    }
}

// Notify helper. `get` returns the `Observable<T>` field on `T`. We re-borrow
// `T` between phases rather than holding one long borrow, so the closures can
// freely mutate `T` (including the observable itself — adds during notify are
// merged, removes are deferred via the `notifying` flag).
pub fn notify<T, F>(owner: &mut T, mut get: F)
where
    T: ?Sized,
    F: FnMut(&mut T) -> &mut Observable<T>,
{
    let mut drained = {
        let obs = get(owner);

        // guard against re-entrant notify on the same observable
        if obs.notifying
        {
            return;
        }

        obs.notifying = true;
        std::mem::take(&mut obs.observers)
    };

    for entry in &mut drained
    {
        (entry.cb)(owner);
    }

    get(owner).merge_after_notify(drained);
}

// Notify variant for owners wrapped in `Arc<RwLock<Box<T>>>` (e.g. `NodeItem`,
// `InstanceItemArc`). The write lock is released between callbacks so a callback
// can freely re-enter the owner via its own `Arc` clone — the lock is only held
// briefly to grab the observer list and during each callback invocation.
pub fn notify_arc<T, F>(owner: &Arc<RwLock<Box<T>>>, mut get: F)
where
    T: ?Sized,
    F: FnMut(&mut T) -> &mut Observable<T>,
{
    let mut drained = {
        let mut w = owner.write().unwrap();
        let obs = get(&mut **w);

        // nothing to call - and pending removes only exist while notifying
        if obs.notifying || obs.observers.is_empty()
        {
            return;
        }
        obs.notifying = true;
        std::mem::take(&mut obs.observers)
    };

    for entry in &mut drained
    {
        let mut w = owner.write().unwrap();
        (entry.cb)(&mut **w);
    }

    let mut w = owner.write().unwrap();
    get(&mut **w).merge_after_notify(drained);
}

// Convenience macro: `notify_observable!(owner, field_name)`
#[macro_export]
macro_rules! notify_observable
{
    ($owner:expr, $field:ident) =>
    {
        $crate::helper::observable::notify($owner, |o| &mut o.$field)
    };
}

// For owners wrapped in `Arc<RwLock<Box<T>>>`
#[macro_export]
macro_rules! notify_observable_arc
{
    ($owner:expr, $field:ident) =>
    {
        $crate::helper::observable::notify_arc($owner, |o| &mut o.$field)
    };
}