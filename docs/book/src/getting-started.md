# Getting Started

Start with a small route that accepts typed data and validates it at the handler
boundary.

```rust
use vyuh::prelude::*;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Validate)]
struct Signup {
    #[validate(email)]
    email: String,
}

#[bundles::route(path = "/users", method = "POST")]
async fn signup(Valid(Data(input)): Valid<Data<Signup>>) -> Result<Data<Signup>, Error> {
    Ok(Data::new(input.as_ref().clone()))
}
```

Register routes, tasks, commands, signals, emitters, services, and assets
through bundles. Use `Site::run` as the normal application entrypoint.

```rust
#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let app = bundles::bundle! {
        signup,
    };

    Site::run(SiteConf::from_env_with_files()?, app).await
}
```
