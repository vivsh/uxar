#![cfg(feature = "postgres")]

use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::Duration;
use vyuh::Data;
use vyuh::emitters::{self, *};

async fn create_site(pool: PgPool) -> vyuh::Site {
    let conf = vyuh::SiteConf {
        log_init: false,
        logging: vyuh::logging::LoggingConf {
            env_prefix: None,
            rules: vec![],
        },
        ..vyuh::SiteConf::from_env().unwrap()
    };
    let parts: Vec<vyuh::bundles::BundlePart> = vec![];
    let bundle = vyuh::bundles::bundle(parts);
    vyuh::Site::test(conf, bundle, pool)
        .await
        .expect("Failed to create test site")
}

#[sqlx::test]
async fn test_periodic(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Clone, schemars::JsonSchema, serde::Serialize, serde::Deserialize)]
    struct Sample;

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    async fn handler(cnt: Arc<AtomicUsize>) -> Data<Sample> {
        cnt.fetch_add(1, Ordering::SeqCst);
        Data::new(Sample)
    }

    let site = create_site(pool).await;
    let emitter = emitters::periodic(
        move |emitters::IterCount(_it): emitters::IterCount| handler(counter_clone.clone()),
        emitters::PeriodicConf {
            interval: Duration::from_millis(100),
            target: emitters::EmitTarget::Signal,
        },
    )?;

    let mut registry = EmitterRegistry::new();
    registry.register(emitter)?;

    let task_site = site.clone();
    let engine = registry.create_engine();
    let run_handle = tokio::spawn(async move { engine.run(task_site).await });

    // Wait for periodic fires (3+ expected in 350ms with 100ms intervals)
    tokio::time::sleep(Duration::from_millis(350)).await;

    let fired_count = counter.load(Ordering::SeqCst);
    assert!(
        fired_count >= 3,
        "Expected at least 3 periodic fires, got {}",
        fired_count
    );

    site.shutdown_and_wait().await;
    let _ = tokio::time::timeout(Duration::from_millis(100), run_handle).await;
    Ok(())
}

#[sqlx::test]
async fn test_cron(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Clone, schemars::JsonSchema, serde::Serialize, serde::Deserialize)]
    struct CronData;

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    async fn handler(cnt: Arc<AtomicUsize>) -> Data<CronData> {
        cnt.fetch_add(1, Ordering::SeqCst);
        Data::new(CronData)
    }

    let site = create_site(pool).await;
    let emitter = emitters::cron(
        move || handler(counter_clone.clone()),
        emitters::CronConf {
            expr: "* * * * * *".into(), // Every second
            target: emitters::EmitTarget::Signal,
        },
    )?;

    let mut registry = EmitterRegistry::new();
    registry.register(emitter)?;

    let task_site = site.clone();
    let engine = registry.create_engine();
    let run_handle = tokio::spawn(async move { engine.run(task_site).await });

    // Wait for cron fires (2+ expected in 2.5 seconds)
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let fired_count = counter.load(Ordering::SeqCst);
    assert!(
        fired_count >= 2,
        "Expected at least 2 cron fires, got {}",
        fired_count
    );

    site.shutdown_and_wait().await;
    let _ = tokio::time::timeout(Duration::from_millis(100), run_handle).await;
    Ok(())
}

#[sqlx::test]
async fn test_pgnotify(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Clone, schemars::JsonSchema, serde::Serialize, serde::Deserialize)]
    struct NotifyData;

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let site = create_site(pool.clone()).await;
    let emitter = emitters::pgnotify(
        move |_s: Data<String>| {
            let cnt = counter_clone.clone();
            async move {
                cnt.fetch_add(1, Ordering::SeqCst);
                Data::new(NotifyData)
            }
        },
        emitters::PgNotifyConf {
            channel: "test_channel".to_string(),
            target: emitters::EmitTarget::Signal,
        },
    )?;

    let mut registry = EmitterRegistry::new();
    registry.register(emitter)?;

    let task_site = site.clone();
    let engine = registry.create_engine();
    let run_handle = tokio::spawn(async move { engine.run(task_site).await });

    // Wait for notifications to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    for _ in 0..3 {
        site.db().send_pgnotify("test_channel", "").await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let fired_count = counter.load(Ordering::SeqCst);
    assert!(
        fired_count >= 3,
        "Expected at least 3 pgnotify fires, got {}",
        fired_count
    );

    site.shutdown_and_wait().await;
    let _ = tokio::time::timeout(Duration::from_millis(100), run_handle).await;
    Ok(())
}
