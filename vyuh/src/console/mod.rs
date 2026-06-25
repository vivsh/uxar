mod api;
mod auth;
mod conf;
mod pages;
mod query;
mod status;
mod types;

use std::time::Duration;

use rust_silos::{Silo, embed_silo};

use crate::{Site, bundles, embed, routes::Methods};

pub use auth::{ConsoleRole, ConsoleUser};
pub use conf::{ConsoleBootstrapMode, ConsoleConf};

pub(crate) use auth::ConsoleRuntime;

const WEB_ASSETS: Silo = embed_silo!("web", force = true);

pub(crate) fn bundle(conf: &ConsoleConf) -> crate::bundles::Bundle {
    web_assets()
        .merge(home_routes(conf))
        .merge(routes().with_prefix(&conf.path))
}

fn web_assets() -> crate::bundles::Bundle {
    bundles::bundle([bundles::asset_dir(embed::Dir::new(WEB_ASSETS.clone()))])
}

fn home_routes(conf: &ConsoleConf) -> crate::bundles::Bundle {
    bundles::bundle([bundles::route(
        pages::overview,
        crate::routes::RouteConf {
            name: "console_home".into(),
            methods: Methods::GET,
            path: conf.path.clone().into(),
            slash: None,
        },
    )])
}

fn routes() -> crate::bundles::Bundle {
    macro_rules! route {
        ($name:literal, $path:literal, $methods:expr, $handler:path $(,)?) => {
            bundles::route(
                $handler,
                crate::routes::RouteConf {
                    name: $name.into(),
                    methods: $methods,
                    path: $path.into(),
                    slash: None,
                },
            )
        };
    }

    bundles::bundle([
        route!(
            "console_overview",
            "/overview",
            Methods::GET,
            pages::overview,
        ),
        route!(
            "console_login_page",
            "/login-page",
            Methods::GET,
            pages::login,
        ),
        route!("console_login", "/login", Methods::GET, api::login),
        route!("console_logout", "/api/logout", Methods::POST, api::logout),
        route!(
            "console_session",
            "/api/session",
            Methods::GET,
            api::session,
        ),
        route!(
            "console_operations",
            "/operations",
            Methods::GET,
            pages::operations,
        ),
        route!(
            "console_operation_detail",
            "/operations/{id}",
            Methods::GET,
            pages::operation_detail,
        ),
        route!("console_tasks", "/tasks", Methods::GET, pages::tasks),
        route!(
            "console_task_detail",
            "/tasks/{id}",
            Methods::GET,
            pages::task_detail,
        ),
        route!(
            "console_api_operations",
            "/api/operations",
            Methods::GET,
            api::operations,
        ),
        route!(
            "console_api_operation_detail",
            "/api/operations/{id}",
            Methods::GET,
            api::operation_detail,
        ),
        route!("console_api_tasks", "/api/tasks", Methods::GET, api::tasks),
        route!(
            "console_api_task_detail",
            "/api/tasks/{id}",
            Methods::GET,
            api::task_detail,
        ),
        route!(
            "console_api_status",
            "/api/status",
            Methods::GET,
            api::status,
        ),
    ])
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
    async fn console_html_pages_and_assets_work() {
        let conf = SiteConf::default()
            .log_init(false)
            .console(ConsoleConf::default().enabled(true));
        let site = Site::build(conf, app_bundle()).await.unwrap();
        let ping_id = site
            .iter_operations()
            .find(|op| op.name == "ping")
            .map(|op| op.id)
            .unwrap();
        let token = site
            .console_runtime()
            .and_then(|runtime| runtime.bootstrap_token())
            .unwrap();
        let client = TestClient::new(site);

        let login = client
            .get(&format!("/_console/login?token={token}"))
            .send()
            .await
            .assert_ok();
        let cookie = login
            .header(header::SET_COOKIE.as_str())
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        let overview = client
            .get("/_console")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(overview.status(), StatusCode::OK, "home page failed");
        let overview = overview.text().await;
        assert!(overview.contains("Overview"));

        let overview = client
            .get("/_console/overview")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(overview.status(), StatusCode::OK, "overview page failed");
        let overview = overview.text().await;
        assert!(overview.contains("Overview"));

        let operations = client
            .get("/_console/operations?kind=route&q=ping")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(
            operations.status(),
            StatusCode::OK,
            "operations page failed"
        );
        let operations = operations.text().await;
        assert!(operations.contains("ping"));

        let operations = client
            .get("/_console/operations")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(
            operations.status(),
            StatusCode::OK,
            "default operations page failed"
        );
        let operations = operations.text().await;
        assert!(operations.contains("ping"));
        assert!(!operations.contains("value=\"none\""));

        let selected = client
            .get(&format!("/_console/operations?selected={ping_id}"))
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(
            selected.status(),
            StatusCode::OK,
            "selected operation page failed"
        );
        let selected = selected.text().await;
        assert!(selected.contains("is-selected"));
        assert!(selected.contains("Methods"));
        assert!(selected.contains("Request"));
        assert!(selected.contains("Response"));

        let tasks = client
            .get("/_console/tasks")
            .header(header::COOKIE.as_str(), &cookie)
            .send()
            .await;
        assert_eq!(tasks.status(), StatusCode::OK, "tasks page failed");
        let tasks = tasks.text().await;
        assert!(tasks.contains("No task records yet."));

        let css = client.get("/assets/css/base.css").send().await;
        assert_eq!(css.status(), StatusCode::OK, "base.css failed");
        assert_eq!(
            css.header(header::CONTENT_TYPE.as_str())
                .and_then(|value| value.to_str().ok()),
            Some("text/css")
        );
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
