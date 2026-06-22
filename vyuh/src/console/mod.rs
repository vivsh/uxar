mod api;
mod auth;
mod conf;
mod query;
mod status;
mod types;

use std::time::Duration;

use axum::routing::{get, post};

use crate::{Site, routes::AxumRouter};

pub use auth::{ConsoleRole, ConsoleUser};
pub use conf::{ConsoleBootstrapMode, ConsoleConf};

pub(crate) use auth::ConsoleRuntime;

pub(crate) fn mount_if_enabled(router: AxumRouter<Site>, conf: &ConsoleConf) -> AxumRouter<Site> {
    if !conf.enabled {
        return router;
    }

    let api = AxumRouter::new()
        .route("/logout", post(api::logout))
        .route("/session", get(api::session))
        .route("/operations", get(api::operations))
        .route("/operations/{id}", get(api::operation_detail))
        .route("/tasks", get(api::tasks))
        .route("/tasks/{id}", get(api::task_detail))
        .route("/status", get(api::status));

    let console = AxumRouter::new()
        .route("/login", get(api::login))
        .nest("/api", api);

    router.nest(&conf.path, console)
}

pub(crate) fn runtime(conf: &ConsoleConf) -> Option<ConsoleRuntime> {
    conf.enabled
        .then(|| ConsoleRuntime::new(Duration::from_secs(conf.bootstrap_token_ttl_seconds)))
}

pub(crate) fn maybe_print_bootstrap_url(site: &Site) {
    let conf = &site.conf().console;
    if !conf.enabled || !should_print(conf, &site.conf().host) {
        return;
    }
    let Some(runtime) = site.console_runtime() else {
        return;
    };
    let Some(token) = runtime.bootstrap_token() else {
        return;
    };
    println!(
        "Vyuh console enabled:\nhttp://{}:{}{}/login?token={}\nToken expires in {} seconds.",
        site.conf().host,
        site.conf().port,
        conf.path,
        token,
        conf.bootstrap_token_ttl_seconds
    );
}

fn should_print(conf: &ConsoleConf, host: &str) -> bool {
    match conf.print_bootstrap_url {
        ConsoleBootstrapMode::Always => true,
        ConsoleBootstrapMode::Disabled => false,
        ConsoleBootstrapMode::LocalOnly => matches!(host, "localhost" | "127.0.0.1" | "::1"),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{StatusCode, header};

    use crate::{
        Site, SiteConf, bundles,
        console::ConsoleConf,
        routes::{Json, Methods, RouteConf},
        testing::TestClient,
    };

    async fn ping() -> Json<&'static str> {
        Json("pong")
    }

    fn app_bundle() -> crate::bundles::Bundle {
        bundles::bundle([bundles::route(
            ping,
            RouteConf {
                name: "ping".into(),
                methods: Methods::GET,
                path: "/ping".into(),
                slash: None,
            },
        )])
    }

    #[tokio::test]
    async fn disabled_console_mounts_no_routes() {
        let site = Site::build(SiteConf::default().log_init(false), app_bundle())
            .await
            .unwrap();
        let client = TestClient::new(site);

        client
            .get("/_console/api/status")
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn console_bootstrap_cookie_authenticates_api() {
        let conf = SiteConf::default()
            .log_init(false)
            .console(ConsoleConf::default().enabled(true));
        let site = Site::build(conf, app_bundle()).await.unwrap();
        let token = site
            .console_runtime()
            .and_then(|runtime| runtime.bootstrap_token())
            .unwrap();
        let client = TestClient::new(site);

        client
            .get("/_console/api/status")
            .send()
            .await
            .assert_status(StatusCode::UNAUTHORIZED);

        let login = client
            .get(&format!("/_console/login?token={token}"))
            .send()
            .await
            .assert_ok();
        client
            .get(&format!("/_console/login?token={token}"))
            .send()
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
        let cookie = login
            .header(header::SET_COOKIE.as_str())
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        client
            .get("/_console/api/operations?kind=route&q=ping")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await
            .assert_ok();
    }

    #[tokio::test]
    async fn console_status_is_cached_within_ttl() {
        let conf = SiteConf::default()
            .log_init(false)
            .console(ConsoleConf::default().enabled(true));
        let site = Site::build(conf, app_bundle()).await.unwrap();
        let runtime = site.console_runtime().unwrap();

        let first = runtime.status(&site, std::time::Duration::from_secs(5));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let second = runtime.status(&site, std::time::Duration::from_secs(5));

        assert_eq!(first.site.uptime_seconds, second.site.uptime_seconds);
    }
}
