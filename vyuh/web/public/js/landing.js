const examples = {
  routes: {
    file: "src/routes/users.rs",
    eyebrow: "Routes",
    title: "Handler-first APIs without hiding the Rust.",
    copy: "Keep validation, permissions, and typed request handling visible where the behavior actually happens.",
    points: [
      "Less boilerplate, more clarity",
      "Compiler-verified request handling",
      "Built for long-term maintainability",
    ],
    code: `use vyuh::prelude::*;

#[derive(Serialize, Deserialize, JsonSchema, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
    name: String,
}

async fn create(
    Permit<User>,
    Valid(Data(input)): Valid<Data<CreateUser>>,
) -> Result<Json<User>, Error> {
    let user = User::create(input).await?;
    Ok(Json(user))
}`,
  },
  tasks: {
    file: "src/tasks/send_welcome.rs",
    eyebrow: "Tasks",
    title: "Move background work into a first-class runtime path.",
    copy: "Tasks run with application context, explicit inputs, and the same dependency model as the web surface.",
    points: [
      "Typed payloads for queued work",
      "Shared app state without globals",
      "Retry and schedule behavior stays visible",
    ],
    code: `use vyuh::prelude::*;

#[derive(Serialize, Deserialize, JsonSchema, Validate)]
struct WelcomeEmail {
    #[validate(email)]
    to: String,
    user_id: Uuid,
}

#[bundles::task(name = "send_welcome")]
async fn send_welcome(
    site: Site,
    Data(input): Data<WelcomeEmail>,
) -> Result<TaskOutcome, Error> {
    let mail = site.service::<Mailer>()?;
    mail.send_template(input.user_id, &input.to).await?;
    TaskOutcome::complete(&"sent").map_err(Error::other)
}`,
  },
  signals: {
    file: "src/signals/user_events.rs",
    eyebrow: "Signals",
    title: "Use typed signals for domain events and lifecycle hooks.",
    copy: "Signals keep local reactions decoupled without turning your app into a hidden event maze.",
    points: [
      "Explicit event contracts",
      "Multiple listeners without request coupling",
      "Good fit for audit, indexing, and follow-up work",
    ],
    code: `use vyuh::prelude::*;

#[derive(Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct UserCreated {
    id: Uuid,
    #[validate(email)]
    email: String,
}

#[bundles::signal]
async fn index_user(
    site: Site,
    Data(event): Data<UserCreated>,
) -> Result<(), Error> {
    let search = site.service::<SearchIndex>()?;
    search.upsert_user(event.id, event.email).await?;
    Ok(())
}`,
  },
  emitters: {
    file: "src/emitters/audit.rs",
    eyebrow: "Emitters",
    title: "Emit structured messages from the same application model.",
    copy: "Emitters make outbound data deliberate, typed, and easy to route into signals.",
    points: [
      "One shape for emitted data",
      "Cron, periodic, and PgNotify sources",
      "Traceable integration points",
    ],
    code: `use vyuh::prelude::*;

#[derive(Serialize, Deserialize, JsonSchema, Validate)]
struct AuditSweep {
    #[validate(length(min = 1))]
    scope: String,
    count: u64,
}

#[bundles::periodic(secs = 60)]
async fn publish_audit_sweep(
    IterCount(count): IterCount,
) -> Data<AuditSweep> {
    Data::new(AuditSweep {
        scope: "users".into(),
        count,
    })
}`,
  },
  commands: {
    file: "src/commands/reindex.rs",
    eyebrow: "Commands",
    title: "Give CLI commands the same app context as production paths.",
    copy: "Commands are operational entry points, not one-off scripts that reimplement setup and wiring.",
    points: [
      "Database and config already loaded",
      "Useful for migrations and maintenance",
      "Easy to document and discover",
    ],
    code: `use vyuh::prelude::*;

#[derive(Serialize, Deserialize, JsonSchema, Validate)]
struct Reindex {
    #[validate(length(min = 1))]
    tenant: String,
}

async fn reindex(
    site: Site,
    Valid(Data(args)): Valid<Data<Reindex>>,
) -> Result<(), Error> {
    let search = site.service::<SearchIndex>()?;
    search.rebuild(args.tenant).await?;
    Ok(())
}`,
  },
  services: {
    file: "src/services/search.rs",
    eyebrow: "Services",
    title: "Run site-lifetime services beside the web application.",
    copy: "Services are durable runtime components for shared clients, caches, coordinators, and workers.",
    points: [
      "Clear startup and shutdown behavior",
      "Shared runtime and observability",
      "No separate mini-framework for workers",
    ],
    code: `use vyuh::prelude::*;
use vyuh::services::{ServiceInstance, ServiceRef};

#[derive(Default)]
struct SearchIndex;

impl vyuh::services::Service for SearchIndex {}

#[bundles::service]
async fn search_index() -> ServiceInstance<SearchIndex> {
    SearchIndex::default().into()
}

#[bundles::route(path = "/search")]
async fn search(
    index: ServiceRef<SearchIndex>,
) -> Json<SearchStats> {
    Json(index.stats().await)
}`,
  },
};

const snippet = document.querySelector("#code-snippet");
const file = document.querySelector("#code-file");
const eyebrow = document.querySelector("#code-eyebrow");
const title = document.querySelector("#code-title");
const copy = document.querySelector("#code-copy");
const pointA = document.querySelector("#code-point-a");
const pointB = document.querySelector("#code-point-b");
const pointC = document.querySelector("#code-point-c");
const tabs = document.querySelectorAll(".landing-code-tab");

function setExample(key) {
  const example = examples[key];
  snippet.textContent = example.code;
  file.textContent = example.file;
  eyebrow.textContent = example.eyebrow;
  title.textContent = example.title;
  copy.textContent = example.copy;
  pointA.textContent = example.points[0];
  pointB.textContent = example.points[1];
  pointC.textContent = example.points[2];
  tabs.forEach((tab) => {
    tab.classList.toggle("is-active", tab.dataset.key === key);
  });
}

tabs.forEach((tab) => {
  tab.addEventListener("click", () => setExample(tab.dataset.key));
});

setExample("routes");
