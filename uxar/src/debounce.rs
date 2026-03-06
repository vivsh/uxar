use std::{fmt, future::Future, pin::Pin, sync::Arc};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use crate::callables::{Callable, CallError, PayloadData};

#[derive(Default, Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum DebounceMode {
    Leading,
    #[default]
    Trailing,
    Both,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct DebounceConf {
    pub mode: DebounceMode,
    pub interval: Duration,
}

/// `SleepUntil(Some(t))` resets the timer to `t`; `SleepUntil(None)` cancels it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceDecision {
    Run,
    Nothing,
    SleepUntil(Option<Instant>),
}

#[derive(Debug, Default)]
pub struct Debouncer {
    mode: DebounceMode,
    interval: Duration,

    running: bool,
    last_trigger: Option<Instant>,   // deadline for trailing = last_trigger + interval
    last_run_start: Option<Instant>, // used to compute cooldown expiry
    pending: bool,                   // a trailing run is queued: set when triggered-while-running, or on every leading fire
}

impl Debouncer {
    pub fn new(conf: DebounceConf) -> Self {
        Self { mode: conf.mode, interval: conf.interval, ..Default::default() }
    }

    /// Called when a new event arrives.
    pub fn trigger(&mut self) -> DebounceDecision {
        let now = Instant::now();
        match self.mode {
            DebounceMode::Leading => {
                if self.running || !self.cooldown_remaining(now).is_zero() {
                    return DebounceDecision::Nothing;
                }
                self.start_run(now);
                DebounceDecision::Run
            }
            DebounceMode::Trailing => {
                self.last_trigger = Some(now);
                if self.running {
                    self.pending = true;
                    return DebounceDecision::Nothing;
                }
                self.schedule_trailing(now)
            }
            DebounceMode::Both => {
                self.last_trigger = Some(now);
                if self.running {
                    // last_trigger is already updated, so the trailing deadline extends naturally
                    return DebounceDecision::Nothing;
                }
                if self.cooldown_remaining(now).is_zero() {
                    self.pending = true; // trailing always follows a leading fire in Both mode
                    self.start_run(now);
                    return DebounceDecision::Run;
                }
                self.schedule_trailing(now)
            }
        }
    }

    /// Called when the scheduled timer fires.
    pub fn awake(&mut self) -> DebounceDecision {
        let now = Instant::now();
        match self.mode {
            DebounceMode::Leading => DebounceDecision::Nothing,
            DebounceMode::Trailing | DebounceMode::Both => {
                if self.running {
                    return DebounceDecision::Nothing;
                }
                self.schedule_trailing(now)
            }
        }
    }

    /// Called when the running action finishes.
    pub fn done(&mut self) -> DebounceDecision {
        self.running = false;
        let now = Instant::now();
        match self.mode {
            DebounceMode::Leading => DebounceDecision::Nothing,
            DebounceMode::Trailing | DebounceMode::Both => {
                if self.pending {
                    self.pending = false;
                    self.schedule_trailing(now)
                } else {
                    DebounceDecision::Nothing
                }
            }
        }
    }

    fn schedule_trailing(&mut self, now: Instant) -> DebounceDecision {
        let Some(deadline) = self.last_trigger.map(|t| t + self.interval) else {
            return DebounceDecision::SleepUntil(None);
        };
        if now >= deadline {
            self.start_run(now);
            DebounceDecision::Run
        } else {
            DebounceDecision::SleepUntil(Some(deadline))
        }
    }

    fn cooldown_remaining(&self, now: Instant) -> Duration {
        self.last_run_start
            .map(|t| (t + self.interval).saturating_duration_since(now))
            .unwrap_or(Duration::ZERO)
    }

    fn start_run(&mut self, now: Instant) {
        self.running = true;
        self.last_run_start = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time;

    const INTERVAL: Duration = Duration::from_millis(100);

    fn leading() -> Debouncer { Debouncer::new(DebounceConf { mode: DebounceMode::Leading, interval: INTERVAL }) }
    fn trailing() -> Debouncer { Debouncer::new(DebounceConf { mode: DebounceMode::Trailing, interval: INTERVAL }) }
    fn both() -> Debouncer { Debouncer::new(DebounceConf { mode: DebounceMode::Both, interval: INTERVAL }) }

    fn deadline_of(d: DebounceDecision) -> Instant {
        match d {
            DebounceDecision::SleepUntil(Some(t)) => t,
            other => panic!("expected SleepUntil(Some(_)), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn leading_idle_fires() {
        time::pause();
        assert_eq!(leading().trigger(), DebounceDecision::Run);
    }

    #[tokio::test]
    async fn leading_trigger_while_running_nothing() {
        time::pause();
        let mut d = leading();
        assert_eq!(d.trigger(), DebounceDecision::Run);
        assert_eq!(d.trigger(), DebounceDecision::Nothing);
        assert_eq!(d.trigger(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn leading_trigger_during_cooldown_nothing() {
        time::pause();
        let mut d = leading();
        d.trigger();
        d.done();
        // still in cooldown
        time::advance(Duration::from_millis(50)).await;
        assert_eq!(d.trigger(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn leading_trigger_after_cooldown_fires() {
        time::pause();
        let mut d = leading();
        d.trigger();
        d.done();
        time::advance(INTERVAL).await;
        assert_eq!(d.trigger(), DebounceDecision::Run);
    }

    #[tokio::test]
    async fn leading_awake_always_nothing() {
        time::pause();
        let mut d = leading();
        assert_eq!(d.awake(), DebounceDecision::Nothing);
        d.trigger();
        assert_eq!(d.awake(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn leading_done_nothing() {
        time::pause();
        let mut d = leading();
        d.trigger();
        assert_eq!(d.done(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn leading_burst_only_first_fires() {
        time::pause();
        let mut d = leading();
        assert_eq!(d.trigger(), DebounceDecision::Run);
        for _ in 0..9 { assert_eq!(d.trigger(), DebounceDecision::Nothing); }
    }

    #[tokio::test]
    async fn trailing_idle_schedules_sleep() {
        time::pause();
        let t0 = Instant::now();
        let mut d = trailing();
        let dec = d.trigger();
        assert_eq!(deadline_of(dec), t0 + INTERVAL);
    }

    #[tokio::test]
    async fn trailing_burst_extends_deadline() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(Duration::from_millis(50)).await;
        let t1 = Instant::now();
        let dec = d.trigger();
        // deadline should reset to the new trigger time
        assert_eq!(deadline_of(dec), t1 + INTERVAL);
    }

    #[tokio::test]
    async fn trailing_awake_at_deadline_fires() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        assert_eq!(d.awake(), DebounceDecision::Run);
    }

    #[tokio::test]
    async fn trailing_awake_before_deadline_resleeps() {
        time::pause();
        let t0 = Instant::now();
        let mut d = trailing();
        d.trigger();
        time::advance(Duration::from_millis(50)).await;
        let dec = d.awake(); // woke up 50ms early
        assert_eq!(deadline_of(dec), t0 + INTERVAL);
    }

    #[tokio::test]
    async fn trailing_done_no_pending_nothing() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        d.awake(); // → Run
        assert_eq!(d.done(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn trailing_trigger_while_running_pending() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        d.awake(); // → Run
        // trigger during run
        let dec = d.trigger();
        assert_eq!(dec, DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn trailing_done_with_pending_schedules_trailing() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        d.awake(); // → Run
        let t1 = Instant::now();
        d.trigger(); // sets pending
        let dec = d.done();
        assert_eq!(deadline_of(dec), t1 + INTERVAL);
    }

    #[tokio::test]
    async fn trailing_burst_during_run_uses_latest_trigger() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        d.awake(); // → Run
        time::advance(Duration::from_millis(10)).await;
        d.trigger(); // t = 110ms
        time::advance(Duration::from_millis(20)).await;
        let t_last = Instant::now(); // t = 130ms
        d.trigger(); // last trigger
        let dec = d.done();
        assert_eq!(deadline_of(dec), t_last + INTERVAL);
    }

    #[tokio::test]
    async fn trailing_awake_stale_while_running_nothing() {
        time::pause();
        let mut d = trailing();
        d.trigger();
        time::advance(INTERVAL).await;
        d.awake(); // → Run
        // stale wake while still running
        assert_eq!(d.awake(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn both_idle_leading_fires() {
        time::pause();
        assert_eq!(both().trigger(), DebounceDecision::Run);
    }

    #[tokio::test]
    async fn both_done_always_schedules_trailing() {
        time::pause();
        let t0 = Instant::now();
        let mut d = both();
        d.trigger(); // → Run + pending=true
        let dec = d.done();
        assert_eq!(deadline_of(dec), t0 + INTERVAL);
    }

    #[tokio::test]
    async fn both_trailing_awake_fires() {
        time::pause();
        let mut d = both();
        let t0 = Instant::now();
        d.trigger();
        d.done(); // → SleepUntil(t0 + 100ms)
        time::advance(INTERVAL).await;
        assert_eq!(d.awake(), DebounceDecision::Run);
    }

    #[tokio::test]
    async fn both_trailing_done_nothing() {
        time::pause();
        let mut d = both();
        d.trigger();
        d.done(); // leading done → trailing sleep
        time::advance(INTERVAL).await;
        d.awake(); // trailing run
        assert_eq!(d.done(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn both_trigger_while_leading_running_nothing() {
        time::pause();
        let mut d = both();
        d.trigger(); // → Run
        assert_eq!(d.trigger(), DebounceDecision::Nothing);
        assert_eq!(d.trigger(), DebounceDecision::Nothing);
    }

    #[tokio::test]
    async fn both_burst_during_leading_extends_trailing_deadline() {
        // triggers during the leading run should push the trailing deadline out
        time::pause();
        let mut d = both();
        d.trigger(); // t=0 → Run
        time::advance(Duration::from_millis(40)).await;
        d.trigger(); // t=40 → Nothing, last_trigger=40ms
        time::advance(Duration::from_millis(30)).await;
        let t_last = Instant::now(); // t=70ms
        d.trigger(); // last_trigger=70ms
        let dec = d.done(); // trailing → deadline = 70ms + 100ms = 170ms
        assert_eq!(deadline_of(dec), t_last + INTERVAL);
    }

    #[tokio::test]
    async fn both_trigger_in_cooldown_trailing() {
        // in cooldown, a new trigger should schedule a trailing run, not a leading one
        time::pause();
        let mut d = both();
        d.trigger(); // leading
        d.done();    // schedules trailing sleep
        time::advance(INTERVAL).await;
        d.awake();   // trailing run
        d.done();    // trailing done → Nothing
        time::advance(Duration::from_millis(20)).await;
        let t1 = Instant::now();
        let dec = d.trigger(); // cooldown still active → SleepUntil
        assert_eq!(deadline_of(dec), t1 + INTERVAL);
    }

    #[tokio::test]
    async fn both_awake_before_deadline_resleeps() {
        time::pause();
        let t0 = Instant::now();
        let mut d = both();
        d.trigger();
        d.done(); // SleepUntil(t0 + 100ms)
        time::advance(Duration::from_millis(40)).await;
        let dec = d.awake(); // stale wake
        assert_eq!(deadline_of(dec), t0 + INTERVAL);
    }

    #[tokio::test]
    async fn both_awake_stale_while_running_nothing() {
        time::pause();
        let mut d = both();
        d.trigger();
        d.done(); // trailing sleep
        time::advance(INTERVAL).await;
        d.awake(); // trailing → Run
        assert_eq!(d.awake(), DebounceDecision::Nothing); // stale wake while trailing run is still active
    }

    #[tokio::test]
    async fn both_second_burst_after_full_cycle() {
        time::pause();
        let mut d = both();
        d.trigger();
        d.done();
        time::advance(INTERVAL).await;
        d.awake(); // trailing run
        d.done();
        // cooldown is anchored to the trailing run_start, advance past it
        time::advance(INTERVAL).await;
        assert_eq!(d.trigger(), DebounceDecision::Run);
    }
}

/// Wraps a [`Callable`] with debounce logic.
/// Calling `trigger` is non-blocking — the actual invocation happens in a background task.
pub struct DebounceCall<C, E = CallError>
where
    C: Send + 'static,
    E: Send + 'static,
{
    latest: Arc<parking_lot::Mutex<Option<C>>>, // the background task always uses the most recent context
    notify: Arc<Notify>,                        // wakes the background task when a new trigger arrives
    _task: Arc<JoinHandle<()>>,
    _e: std::marker::PhantomData<E>,
}

impl<C, E> Clone for DebounceCall<C, E>
where
    C: Send + 'static,
    E: Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            latest: Arc::clone(&self.latest),
            notify: Arc::clone(&self.notify),
            _task: Arc::clone(&self._task),
            _e: std::marker::PhantomData,
        }
    }
}

impl<C, E> DebounceCall<C, E>
where
    C: Send + Clone + 'static,
    E: fmt::Debug + From<CallError> + Send + 'static,
{
    pub fn new(conf: DebounceConf, callable: Callable<C, E>) -> Self {
        let latest = Arc::new(parking_lot::Mutex::new(None::<C>));
        let notify = Arc::new(Notify::new());
        let task = tokio::spawn(debounce_task(
            conf, callable, Arc::clone(&latest), Arc::clone(&notify),
        ));
        Self { latest, notify, _task: Arc::new(task), _e: std::marker::PhantomData }
    }

    /// Stores the latest context and wakes the background task. Never blocks.
    pub fn trigger(&self, ctx: C) {
        *self.latest.lock() = Some(ctx);
        self.notify.notify_one();
    }
}

async fn debounce_task<C, E>(
    conf: DebounceConf,
    callable: Callable<C, E>,
    latest: Arc<parking_lot::Mutex<Option<C>>>,
    notify: Arc<Notify>,
)
where
    C: Send + Clone + 'static,
    E: fmt::Debug + From<CallError> + Send + 'static,
{
    let mut state = Debouncer::new(conf);
    let mut timer_active = false;
    let sleep = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(sleep);

    // call_fut holds the in-flight handler; pending() when idle so the select
    // branch is always valid — call_active gates whether it's actually polled.
    let mut call_active = false;
    let mut call_fut: Pin<Box<dyn Future<Output = Result<PayloadData, E>> + Send>> =
        Box::pin(std::future::pending());

    loop {
        let decision = tokio::select! {
            () = notify.notified() => state.trigger(),
            () = &mut sleep, if timer_active => {
                timer_active = false;
                state.awake()
            }
            result = &mut call_fut, if call_active => {
                call_active = false;
                if let Err(e) = result {
                    tracing::error!("debounce handler error: {:?}", e);
                }
                state.done()
            }
        };

        match decision {
            DebounceDecision::Run => {
                timer_active = false;
                let ctx = latest.lock().clone();
                if let Some(ctx) = ctx {
                    call_fut = callable.call(ctx);
                    call_active = true;
                } else {
                    // ctx missing despite Run decision — reset state to avoid getting stuck
                    tracing::warn!("debounce Run with no queued context");
                    let _ = state.done();
                }
            }
            DebounceDecision::SleepUntil(Some(t)) => { sleep.as_mut().reset(t); timer_active = true; }
            DebounceDecision::SleepUntil(None)     => { timer_active = false; }
            DebounceDecision::Nothing => {}
        }
    }
}

#[cfg(test)]
mod call_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use tokio::time;

    const INTERVAL: Duration = Duration::from_millis(100);

    // callable that just counts how many times it was invoked
    fn counter_call(counter: Arc<AtomicUsize>) -> Callable<(), CallError> {
        Callable::new(move || {
            let c = Arc::clone(&counter);
            async move { c.fetch_add(1, Ordering::SeqCst); }
        })
    }

    fn make(mode: DebounceMode, callable: Callable<(), CallError>) -> DebounceCall<(), CallError> {
        DebounceCall::new(DebounceConf { mode, interval: INTERVAL }, callable)
    }

    // sleep(ZERO) flushes the timer wheel so tasks whose sleeps expired via advance() get
    // scheduled on the executor; the yield_nows then let them run to completion.
    async fn settle() {
        tokio::time::sleep(Duration::ZERO).await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn leading_fires_immediately() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Leading, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn leading_burst_fires_once() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Leading, counter_call(Arc::clone(&c)));
        dc.trigger(());
        dc.trigger(());
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn leading_cooldown_blocks() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Leading, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
        time::advance(Duration::from_millis(50)).await;
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1, "cooldown must block re-fire");
    }

    #[tokio::test]
    async fn leading_fires_after_cooldown() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Leading, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        time::advance(INTERVAL).await;
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn trailing_defers_until_quiet() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 0, "must not fire before interval");
        time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn trailing_burst_fires_once_after_last() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        dc.trigger(());
        time::advance(Duration::from_millis(40)).await;
        dc.trigger(());
        time::advance(Duration::from_millis(40)).await;
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 0, "must not fire mid-burst");
        time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn trailing_sequential_triggers_each_fire() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        for _ in 0..3 {
            dc.trigger(());
            settle().await; // let the task record the trigger before advancing time
            time::advance(INTERVAL).await;
            settle().await;
        }
        assert_eq!(c.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn trailing_no_fire_without_trigger() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let _dc = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        time::advance(INTERVAL * 10).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn both_leading_fires_immediately() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Both, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1, "leading must fire immediately");
    }

    #[tokio::test]
    async fn both_trailing_fires_after_interval() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Both, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
        time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 2, "trailing must follow leading");
    }

    #[tokio::test]
    async fn both_burst_fires_leading_and_one_trailing() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Both, counter_call(Arc::clone(&c)));
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1); // leading
        time::advance(Duration::from_millis(40)).await;
        dc.trigger(());
        time::advance(Duration::from_millis(40)).await;
        dc.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1, "no extra fires during burst");
        time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 2, "trailing fires once after quiet");
    }

    #[tokio::test]
    async fn both_no_fire_without_trigger() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let _dc = make(DebounceMode::Both, counter_call(Arc::clone(&c)));
        time::advance(INTERVAL * 10).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn clone_shares_background_task() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc1 = make(DebounceMode::Leading, counter_call(Arc::clone(&c)));
        let dc2 = dc1.clone();
        dc1.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
        // still within the cooldown window
        dc2.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1, "clone must share cooldown");
        time::advance(INTERVAL).await;
        dc2.trigger(());
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 2, "clone fires after shared cooldown expires");
    }

    #[tokio::test]
    async fn trailing_runs_are_sequential() {
        time::pause();
        let running = Arc::new(AtomicBool::new(false));
        let overlap_seen = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicUsize::new(0));
        let r = Arc::clone(&running);
        let o = Arc::clone(&overlap_seen);
        let cnt = Arc::clone(&counter);
        let callable: Callable<(), CallError> = Callable::new(move || {
            let r = Arc::clone(&r);
            let o = Arc::clone(&o);
            let cnt = Arc::clone(&cnt);
            async move {
                if r.swap(true, Ordering::SeqCst) {
                    o.store(true, Ordering::SeqCst);
                }
                cnt.fetch_add(1, Ordering::SeqCst);
                r.store(false, Ordering::SeqCst);
            }
        });
        let dc = make(DebounceMode::Trailing, callable);
        for _ in 0..3 {
            dc.trigger(());
            settle().await; // let the task record the trigger before advancing time
            time::advance(INTERVAL).await;
            settle().await;
        }
        assert_eq!(counter.load(Ordering::SeqCst), 3, "handler should run once per quiet period");
        assert!(!overlap_seen.load(Ordering::SeqCst), "runs must never overlap");
    }

    #[tokio::test]
    async fn drop_does_not_panic() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        dc.trigger(());
        drop(dc);
        settle().await;
    }

    #[tokio::test]
    async fn drop_last_clone_releases_task() {
        time::pause();
        let c = Arc::new(AtomicUsize::new(0));
        let dc1 = make(DebounceMode::Trailing, counter_call(Arc::clone(&c)));
        let dc2 = dc1.clone();
        drop(dc1);
        // dc2 still holds the task alive
        dc2.trigger(());
        settle().await; // let the task record the trigger before advancing time
        time::advance(INTERVAL).await;
        settle().await;
        assert_eq!(c.load(Ordering::SeqCst), 1);
        drop(dc2);
        settle().await;
    }
}

