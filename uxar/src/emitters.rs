use std::{
    any::TypeId,
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};
use serde::{Deserialize, Serialize};


use crate::{
    Site,
    callables::{
        self, CallError, Callable, IntoArgPart, IntoPayloadData, PatchOp, Payload, PayloadData,
        Payloadable, Specable,
    },
};

pub struct IterCount(pub usize);

impl IntoArgPart for IterCount {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
}

impl callables::FromContext<EmitterContext> for IterCount {
    fn from_context(ctx: EmitterContext) -> Result<Self, CallError> {
        Ok(IterCount(ctx.iter_count))
    }
}

pub struct IterInstant(pub Option<tokio::time::Instant>);

impl IntoArgPart for IterInstant {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
}

impl callables::FromContext<EmitterContext> for IterInstant {
    fn from_context(ctx: EmitterContext) -> Result<Self, CallError> {
        Ok(IterInstant(ctx.last_time))
    }
}

pub struct EmitterContext {
    site: Site,
    iter_count: usize,
    last_time: Option<tokio::time::Instant>,
    payload: PayloadData,
}

pub type EmitterHandler = Callable<EmitterContext>;

impl IntoPayloadData for EmitterContext {
    fn into_payload_data(self) -> PayloadData {
        self.payload
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy, Default, Deserialize, Serialize)]
pub enum EmitTarget {
    #[default]
    Signal,
    Task,
}

#[derive(Debug, thiserror::Error)]
pub enum EmitterError {
    #[error("Emitter payload type mismatch")]
    PgNotifyError(#[from] crate::db::DbError),

    #[error("Cron expression error: {0}")]
    CronError(#[from] cron::error::Error),

    #[error("Emitter with the given type already exists")]
    AlreadyExists,

    #[error("Other error: {0}")]
    OtherError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error(transparent)]
    CallError(#[from] CallError),
}

#[derive(Debug)]
pub struct Emitter {
    type_id: TypeId,
    target: EmitTarget,
    source: EmitterSource,
}

impl Emitter {
    fn key(&self) -> (TypeId, u8) {
        (self.type_id, self.source.discriminant())
    }

    pub fn operation(&self) -> callables::Operation {
        let specs = self.source.spec();
        let (kind, config) = match &self.source {
            EmitterSource::Cron { schedule, .. } => (
                callables::OperationKind::Cron,
                Some(serde_json::json!({ "expr": schedule.to_string() }))
            ),
            EmitterSource::Periodic { interval, .. } => (
                callables::OperationKind::Periodic,
                Some(serde_json::json!({ "interval_secs": interval.as_secs() }))
            ),
            EmitterSource::PgNotify { channel, .. } => (
                callables::OperationKind::PgNotify,
                Some(serde_json::json!({ "channel": channel }))
            ),
            EmitterSource::Beacon { channel, .. } => (
                callables::OperationKind::Signal,
                Some(serde_json::json!({ "channel": channel }))
            ),
        };        
        callables::Operation::from_specs(kind, specs).with_conf(&config)    
    }
}

#[repr(u8)]
enum EmitterSource {
    Cron {
        schedule: cron::Schedule,
        handler: EmitterHandler,
    },
    Periodic {
        interval: tokio::time::Duration,
        handler: EmitterHandler,
    },
    PgNotify {
        channel: String,
        handler: EmitterHandler,
    },
    Beacon {
        channel: String,
        handler: EmitterHandler,
    },
}

impl EmitterSource {
    pub fn discriminant(&self) -> u8 {
        match self {
            EmitterSource::Cron { .. } => 0,
            EmitterSource::Periodic { .. } => 1,
            EmitterSource::PgNotify { .. } => 2,
            EmitterSource::Beacon { .. } => 3,
        }
    }

    fn spec(&self) -> &callables::CallSpec {
        match self {
            EmitterSource::Cron { handler, .. } => handler.inspect(),
            EmitterSource::Periodic { handler, .. } => handler.inspect(),
            EmitterSource::PgNotify { handler, .. } => handler.inspect(),
            EmitterSource::Beacon { handler, .. } => handler.inspect(),
        }
    }
}

impl std::fmt::Debug for EmitterSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitterSource::Cron { schedule, .. } => f.debug_tuple("Cron").field(schedule).finish(),
            EmitterSource::Periodic { interval, .. } => {
                f.debug_tuple("Periodic").field(interval).finish()
            }
            EmitterSource::PgNotify { channel, .. } => {
                f.debug_tuple("PgNotify").field(channel).finish()
            }
            EmitterSource::Beacon { channel, .. } => {
                f.debug_tuple("Beacon").field(channel).finish()
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CronConf {
    pub expr: String,
    pub target: EmitTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PeriodicConf {
    pub interval: tokio::time::Duration,
    pub target: EmitTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PgNotifyConf {
    pub channel: String,
    pub target: EmitTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BeaconConf {
    pub channel: String,
    pub target: EmitTarget,
}

pub(crate) fn beacon<H, Args, O>(handler: H, options: BeaconConf) -> Result<Emitter, EmitterError>
where
    O: Payloadable,
    H: callables::Specable<Args, Output = Payload<O>> + Send + Sync + 'static,
    Args: callables::FromContext<EmitterContext> + callables::IntoArgSpecs,
{
    let wrapper = Callable::new(handler);
    Ok(Emitter {
        type_id: TypeId::of::<O>(),
        target: options.target,
        source: EmitterSource::Beacon {
            channel: options.channel,
            handler: wrapper,
        },
    })
}

pub fn cron<H, Args, O>(handler: H, options: CronConf) -> Result<Emitter, EmitterError>
where
    O: Payloadable,
    H: callables::Specable<Args, Output = Payload<O>> + Send + Sync + 'static,
    Args: callables::FromContext<EmitterContext> + callables::IntoArgSpecs,
{
    let wrapper = Callable::new(handler);
    Ok(Emitter {
        type_id: TypeId::of::<O>(),
        target: options.target,
        source: EmitterSource::Cron {
            schedule: options.expr.parse::<cron::Schedule>()?,
            handler: wrapper,
        },
    })
}

pub fn periodic<H, Args, O>(handler: H, options: PeriodicConf) -> Result<Emitter, EmitterError>
where
    O: Payloadable,
    H: callables::Specable<Args, Output = Payload<O>> + Send + Sync + 'static,
    Args: callables::FromContext<EmitterContext> + callables::IntoArgSpecs,
{
    let wrapper = Callable::new(handler);
    Ok(Emitter {
        type_id: TypeId::of::<O>(),
        target: options.target,
        source: EmitterSource::Periodic {
            interval: options.interval,
            handler: wrapper,
        },
    })
}

pub fn pgnotify<H, Args, O>(handler: H, options: PgNotifyConf) -> Result<Emitter, EmitterError>
where
    O: Payloadable,
    H: callables::Specable<Args, Output = Payload<O>> + Send + Sync + 'static,
    Args: callables::FromContext<EmitterContext> + callables::IntoArgSpecs,
{
    let wrapper = Callable::new(handler);
    Ok(Emitter {
        type_id: TypeId::of::<O>(),
        target: options.target,
        source: EmitterSource::PgNotify {
            channel: options.channel,
            handler: wrapper,
        },
    })
}

#[derive(Clone)]
pub struct EmitterRegistry {
    sources: HashMap<(TypeId, u8), Arc<Emitter>>,
}

impl EmitterRegistry {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    pub fn iter_emitters(&self) -> impl Iterator<Item = &Emitter> {
        self.sources.values().map(|e| e.as_ref())
    }

    pub fn register(&mut self, emitter: Emitter) -> Result<(), EmitterError> {
        let key = emitter.key();
        if self.sources.contains_key(&key) {
            return Err(EmitterError::AlreadyExists);
        }
        self.sources.insert(key, Arc::new(emitter));
        Ok(())
    }

    pub fn merge(&mut self, other: EmitterRegistry) -> Result<(), EmitterError> {
        for (key, emitter) in other.sources {
            if self.sources.contains_key(&key) {
                return Err(EmitterError::AlreadyExists);
            }
            self.sources.insert(key, emitter);
        }
        Ok(())
    }

    pub fn create_engine(&self) -> EmitterEngine {
        EmitterEngine {
            sources: self.sources.clone(),
        }
    }
}

#[derive(Clone)]
pub struct EmitterEngine {
    sources: HashMap<(TypeId, u8), Arc<Emitter>>,
}

impl EmitterEngine {
    async fn dispatch(&self, site: &Site, payload: PayloadData, target: EmitTarget) {
        if let Err(err) = site.dispatch_payload(payload, target).await {
            tracing::error!("Error dispatching emitter payload: {}", err);
        }
    }

    pub async fn run(self, site: Site) -> Result<(), EmitterError> {
        let mut timer_tasks = TimerQueue::new();
        let shutdown = site.shutdown_notifier();
        let mut notify_tasks: HashMap<String, Vec<NotifyWork>> = HashMap::new();

        for ((type_id, _), emitter) in &self.sources {
            match &emitter.source {
                EmitterSource::Periodic {
                    interval: dur,
                    handler,
                } => {
                    let work = TimerWork::new(
                        type_id.clone(),
                        TimerKind::Interval(dur.clone()),
                        handler.clone(),
                        emitter.target,
                    );
                    timer_tasks.push(work);
                }
                EmitterSource::Cron { schedule, handler } => {
                    let work = TimerWork::new(
                        type_id.clone(),
                        TimerKind::Schedule(schedule.clone()),
                        handler.clone(),
                        emitter.target,
                    );
                    timer_tasks.push(work);
                }
                EmitterSource::PgNotify { channel, handler } => {
                    notify_tasks
                        .entry(channel.clone())
                        .or_default()
                        .push(NotifyWork {
                            type_id: type_id.clone(),
                            handler: handler.clone(),
                            target: emitter.target,
                            iter_count: 0,
                            last_time: None,
                        });
                }
                EmitterSource::Beacon { channel, handler } => {
                    unimplemented!()
                }
            }
        }
        let topics = notify_tasks.keys().cloned().collect::<Vec<_>>();
        let mut receiver = site.consume_notify(&topics).await?;
        let dummy_data = PayloadData::new(String::new());
        loop {

            tokio::select! {
                Some(work) = timer_tasks.pop() => {
                    let ctx = EmitterContext{
                        site: site.clone(),
                        payload: dummy_data.clone(),
                        iter_count: work.iter_count,
                        last_time: work.last_time,
                    };
                    let target = work.target.clone();
                    match work.producer.call(ctx).await{
                        Ok(payload)=>{
                            self.dispatch(&site, payload, target).await;
                        }
                        Err(err)=>{
                            tracing::error!("Error calling emitter handler: {}", err);
                        }
                    }
                    timer_tasks.push(work);
                },
                _=shutdown.notified()=>{
                    tracing::info!("SignalEngine shutting down");
                    break;
                }
                Some(notif) = receiver.recv()=>{
                    if let Some(signal_names) = notify_tasks.get_mut(notif.channel.as_str()){
                        for notify_work in signal_names {
                            let result = notify_work.handler.call(EmitterContext{
                                site: site.clone(),
                                payload: PayloadData::new(notif.payload.clone()),
                                iter_count: notify_work.iter_count,
                                last_time: notify_work.last_time,
                            }).await;
                            timer_tasks.touch(notify_work.type_id);
                            let target = notify_work.target.clone();
                            match result {
                                Ok(payload)=> {
                                    self.dispatch(&site, payload, target).await;
                                },
                                Err(err)=>{
                                    tracing::error!("Error parsing pgnotify payload for channel '{}': {}", notif.channel, err);
                                }
                            }
                            notify_work.update();
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct NotifyWork {
    type_id: TypeId,
    handler: EmitterHandler,
    target: EmitTarget,
    iter_count: usize,
    last_time: Option<tokio::time::Instant>,
}

impl NotifyWork {
    fn new(type_id: TypeId, handler: EmitterHandler, target: EmitTarget) -> Self {
        Self {
            type_id,
            handler,
            target,
            iter_count: 0,
            last_time: None,
        }
    }

    fn update(&mut self) {
        self.iter_count = self.iter_count.wrapping_add(1);
        self.last_time = Some(tokio::time::Instant::now());
    }
}

#[derive(Clone, Debug)]
enum TimerKind {
    Schedule(cron::Schedule),
    Interval(tokio::time::Duration),
}

struct TimerWork {
    type_id: TypeId,
    last: Option<tokio::time::Instant>,
    kind: TimerKind,
    target: EmitTarget,
    producer: EmitterHandler,
    deadline: tokio::time::Instant,
    iter_count: usize,
    last_time: Option<tokio::time::Instant>,
}

impl TimerWork {
    fn new(type_id: TypeId, kind: TimerKind, producer: EmitterHandler, target: EmitTarget) -> Self {
        let deadline = Self::make_deadline(None, &kind);
        Self {
            type_id,
            last: None,
            kind,
            producer,
            target,
            deadline,
            iter_count: 0,
            last_time: None,
        }
    }

    fn update_deadline(&mut self, last: tokio::time::Instant) -> bool {
        if self.last.map(|l| l >= last).unwrap_or(false) {
            return false;
        }
        self.last = Some(last);
        self.deadline = Self::make_deadline(self.last, &self.kind);
        true
    }

    fn update(&mut self) {
        self.iter_count = self.iter_count.wrapping_add(1);
        self.last_time = Some(tokio::time::Instant::now());
        self.last = Some(tokio::time::Instant::now());
        self.deadline = Self::make_deadline(self.last, &self.kind);
    }

    fn make_deadline(last: Option<tokio::time::Instant>, kind: &TimerKind) -> tokio::time::Instant {
        match kind {
            TimerKind::Interval(dur) => {
                if let Some(last) = last {
                    last + *dur
                } else {
                    tokio::time::Instant::now()
                }
            }
            TimerKind::Schedule(schedule) => {
                let now = chrono::Utc::now();
                let next = schedule.upcoming(chrono::Utc).next().unwrap_or_default();
                let duration = next.signed_duration_since(now).to_std().unwrap_or_default();
                if let Some(_last) = last {
                    tokio::time::Instant::now() + duration
                } else {
                    tokio::time::Instant::now()
                }
            }
        }
    }
}

impl Eq for TimerWork {}

impl PartialEq for TimerWork {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}

impl Ord for TimerWork {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.deadline.cmp(&self.deadline)
    }
}

impl PartialOrd for TimerWork {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct TimerQueue {
    heap: BinaryHeap<TimerWork>,
    timeout_map: HashMap<TypeId, tokio::time::Instant>,
    notifier: tokio::sync::Notify,
}

impl TimerQueue {
    fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            timeout_map: HashMap::new(),
            notifier: tokio::sync::Notify::new(),
        }
    }

    fn push(&mut self, work: TimerWork) {
        let key = work.type_id;
        self.heap.push(work);
        self.notifier.notify_one();
    }

    /// It is intended to be used as a way to update the deadline when work is done elsewhere
    /// So the deadline may need to be adjusted based on the new time
    fn touch(&mut self, type_id: TypeId) {
        let key = type_id;
        self.timeout_map.insert(key, tokio::time::Instant::now());
    }

    async fn pop(&mut self) -> Option<TimerWork> {
        loop {
            if let Some(work) = self.heap.peek() {
                let now = tokio::time::Instant::now();

                if work.deadline <= now {
                    let mut popped = if let Some(p) = self.heap.pop() {
                        p
                    } else {
                        continue;
                    };
                    if let Some(updated) = self.timeout_map.remove(&popped.type_id) {
                        if popped.update_deadline(updated) {
                            self.heap.push(popped);
                            continue;
                        }
                    }
                    popped.update();
                    return Some(popped);
                }

                let wait = work.deadline - now;
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {},
                    _ = self.notifier.notified() => {},
                }
            } else {
                self.notifier.notified().await;
            }
        }
    }
}
