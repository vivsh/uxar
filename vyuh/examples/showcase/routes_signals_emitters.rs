use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, Site, Valid, Validate, bundles};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Validate)]
struct Signup {
    #[validate(email)]
    email: String,

    #[validate(min_length = 3, max_length = 80)]
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct UserCreated {
    id: i64,
    email: String,
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct SystemPulse {
    project: String,
}

#[bundles::route(path = "/users", method = "POST")]
async fn signup(Valid(Data(input)): Valid<Data<Signup>>) -> Result<Data<UserCreated>, Error> {
    Ok(Data::new(UserCreated {
        id: 1,
        email: input.email.clone(),
        name: input.name.clone(),
    }))
}

#[bundles::cron(expr = "0 */5 * * * *")]
async fn heartbeat(site: Site) -> Data<SystemPulse> {
    Data::new(SystemPulse {
        project: site.project_dir().display().to_string(),
    })
}

#[bundles::signal]
async fn record_heartbeat(Data(pulse): Data<SystemPulse>) {
    println!("heartbeat for {}", pulse.project);
}

fn main() {
    let bundle = bundles::bundle! {
        signup,
        heartbeat,
        record_heartbeat,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Vyuh Example")
            .version("0.2.3")
            .description("Handler-first routes, typed data, emitters, signals, and OpenAPI.")
            .spec("/openapi.json"),
    )
    .with_prefix("/api");

    assert_eq!(
        bundle.reverse("signup", &[]),
        Some("/api/users".to_string())
    );
    println!("showcase bundle ready: POST /api/users, OpenAPI at /api/openapi.json");
}
