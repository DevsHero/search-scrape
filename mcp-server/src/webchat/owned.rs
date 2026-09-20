//! Owned-localhost webchat transport over the existing stateful CDP owner.
use super::*;
use crate::cdp::{lease, state};
use anyhow::{anyhow, bail, Result};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub protocol_version: String,
    pub name: String,
    pub allowed_origins: Vec<String>,
    pub profile_id: String,
    pub auth_state_requirement: String,
    pub components: WebchatComponents,
    pub recipe: Option<WebchatRecipe>,
    pub text: bool,
    pub upload: bool,
    pub image: bool,
    pub bridge: bool,
    pub billing_visibility: String,
    pub enabled: bool,
}
impl ProviderDescriptor {
    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != "webchat_provider_descriptor_v1"
            || self.name.is_empty()
            || self.profile_id != "owned_fixture"
            || self.allowed_origins.len() != 1
            || self.auth_state_requirement != "session_ready"
            || !self.text
            || self.upload
            || self.image
            || self.billing_visibility != "unknown"
        {
            bail!("unsupported owned text descriptor");
        }
        owned_origin(&self.allowed_origins[0])?;
        if self.components.origin != self.allowed_origins[0] {
            bail!("component origin mismatch");
        }
        self.components.validate().map_err(|e| anyhow!("{e:?}"))?;
        if self.components.protocol_version != WB03_COMPONENTS_PROTOCOL_V1
            || self
                .components
                .locators
                .iter()
                .any(|l| l.kind != "css" || l.value.len() > 256)
        {
            bail!("only bounded declared CSS locators supported");
        }
        if let Some(recipe) = &self.recipe {
            let params = BTreeMap::from([("prompt".into(), "probe".into())]);
            recipe
                .validate_replay(&self.components.origin, &params, &self.components)
                .map_err(|e| anyhow!("{e:?}"))?;
        }
        Ok(())
    }
}
fn owned_origin(origin: &str) -> Result<()> {
    let url = url::Url::parse(origin)?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("owned literal loopback origin required");
    }
    Ok(())
}
struct Job {
    readiness: BrowserSessionReadiness,
    descriptor: ProviderDescriptor,
    turns: BTreeMap<String, (String, Option<WebchatMessage>)>,
    messages: u64,
}
struct Runtime {
    root: PathBuf,
    descriptor: ProviderDescriptor,
    job: Option<Job>,
}
type Shared = Arc<Mutex<Runtime>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    action: String,
    job_id: String,
    lease_id: Option<String>,
    turn_id: Option<String>,
    prompt: Option<String>,
}
async fn descriptor(State(runtime): State<Shared>) -> Json<Value> {
    let r = runtime.lock().await;
    let active_state = match &r.job {
        Some(job)
            if job.readiness.is_ready()
                && job.turns.values().any(|(_, message)| message.is_some()) =>
        {
            "tested"
        }
        Some(job) if job.readiness.is_ready() => "ready",
        _ => "unknown",
    };
    let matches_saved = r.job.as_ref().is_some_and(|job| {
        serde_json::to_value(&job.descriptor).ok() == serde_json::to_value(&r.descriptor).ok()
    });
    let saved_state = if !r.descriptor.enabled {
        "disabled"
    } else if matches_saved && active_state != "unknown" {
        active_state
    } else {
        "saved"
    };
    Json(
        json!({"descriptor":r.descriptor,"state":saved_state,"active_job_state":active_state,
        "active_recipe_revision":r.job.as_ref().and_then(|job|job.descriptor.recipe.as_ref().map(|recipe|recipe.revision))}),
    )
}
async fn command(
    State(runtime): State<Shared>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Browser cross-origin callers cannot supply this non-simple header without
    // a preflight; this dedicated localhost service grants no CORS permission.
    if headers.get("x-scout-owned").and_then(|h| h.to_str().ok()) != Some("1")
        || headers.contains_key("origin")
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error":"local explicit client required"})),
        ));
    }
    let mut runtime = runtime.lock().await;
    perform(&mut runtime, command)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::CONFLICT, Json(json!({"error":e.to_string()}))))
}
async fn page_value(page: &chromiumoxide::Page, script: String) -> Result<Value> {
    Ok(page.evaluate(script).await?.into_value::<Value>()?)
}
async fn perform(runtime: &mut Runtime, command: Command) -> Result<Value> {
    let profile = runtime.root.join("profile");
    std::fs::create_dir_all(&profile)?;
    let profile_key = lease::profile_key_for(&profile);
    if command.action == "open" {
        if runtime.job.is_some() {
            bail!("profile already leased; use existing lease or close it explicitly");
        }
        runtime.descriptor.validate()?;
        if !runtime.descriptor.enabled {
            bail!("descriptor disabled");
        }
        let claimed = lease::acquire(&profile_key, &command.job_id, Duration::from_secs(600))
            .map_err(anyhow::Error::msg)?;
        let mut guard = state::session_lock().lock().await;
        if let Err(e) = state::ensure_profile_session(&mut guard, profile, false).await {
            lease::release(&profile_key, &command.job_id).map_err(anyhow::Error::msg)?;
            return Err(e);
        }
        let mut readiness = BrowserSessionReadiness::needs_user(
            &claimed.lease_id,
            "owned_fixture",
            &command.job_id,
            &runtime.descriptor.components.origin,
        );
        let page = &guard.as_ref().unwrap().page;
        if page
            .goto(&runtime.descriptor.components.origin)
            .await
            .is_err()
        {
            if let Some(session) = guard.take() {
                session.shutdown().await;
            }
            lease::release(&profile_key, &command.job_id).map_err(anyhow::Error::msg)?;
            bail!("owned page unavailable");
        }
        readiness.transition_login_required();
        let result = serde_json::to_value(&readiness)?;
        runtime.job = Some(Job {
            readiness,
            descriptor: runtime.descriptor.clone(),
            turns: BTreeMap::new(),
            messages: 0,
        });
        return Ok(result);
    }
    let job = runtime
        .job
        .as_mut()
        .ok_or_else(|| anyhow!("no active job"))?;
    if job.readiness.job_id != command.job_id
        || command.lease_id.as_deref() != Some(&job.readiness.lease_id)
    {
        bail!("job/lease authority mismatch; no close or steal");
    }
    if command.action == "close" {
        let mut guard = state::session_lock().lock().await;
        if let Some(session) = guard.take() {
            session.shutdown().await;
        }
        lease::release(&profile_key, &command.job_id).map_err(anyhow::Error::msg)?;
        runtime.job = None;
        return Ok(json!({"state":"closed"}));
    }
    if now_ms() >= job.readiness.expires_at_ms {
        job.readiness.mark_expired();
        bail!("lease expired; holder must close");
    }
    let guard = state::session_lock().lock().await;
    let page = &guard
        .as_ref()
        .ok_or_else(|| anyhow!("browser unavailable"))?
        .page;
    let origin = page_value(page, "location.origin".into()).await?;
    if origin.as_str() != Some(&job.descriptor.components.origin) {
        bail!("origin changed; stopped");
    }
    match command.action.as_str() {
        "record" => {
            let revision = runtime
                .descriptor
                .recipe
                .as_ref()
                .map(|r| r.revision + 1)
                .unwrap_or(1);
            let recipe = WebchatRecipe {
                protocol_version: WB05_RECIPE_PROTOCOL_V1.into(),
                recipe_id: "owned-text".into(),
                revision,
                required_origin: job.descriptor.components.origin.clone(),
                component_descriptor_digest: job.descriptor.components.digest(),
                steps: vec![
                    RecipeStep {
                        action: "navigate".into(),
                        locator_role: None,
                        parameter: None,
                    },
                    RecipeStep {
                        action: "insert".into(),
                        locator_role: Some(ComponentRole::Composer),
                        parameter: Some("${prompt}".into()),
                    },
                    RecipeStep {
                        action: "submit".into(),
                        locator_role: Some(ComponentRole::Submit),
                        parameter: None,
                    },
                    RecipeStep {
                        action: "await_terminal".into(),
                        locator_role: Some(ComponentRole::AssistantMessages),
                        parameter: None,
                    },
                ],
                parameter_schema: vec!["prompt".into()],
                preconditions: vec!["c06.session_ready".into()],
                postconditions: vec!["wb03.completed".into()],
                approval_points: vec!["explicit send".into()],
            };
            WebchatRecipe::sanitized(&recipe.steps).map_err(|e| anyhow!("{e:?}"))?;
            runtime.descriptor.recipe = Some(recipe.clone());
            let bytes = serde_json::to_vec_pretty(&runtime.descriptor)?;
            let path = runtime.root.join("provider.json");
            let temp = runtime.root.join("provider.tmp");
            std::fs::write(&temp, bytes)?;
            std::fs::rename(temp, path)?;
            Ok(
                json!({"recipe":recipe,"active_job_recipe_revision":job.descriptor.recipe.as_ref().map(|r|r.revision),"digest":format!("sha256:{}",hex::encode(sha2::Sha256::digest(serde_json::to_vec(&recipe)?)))}),
            )
        }
        "status" => Ok(
            json!({"readiness":job.readiness,"browser_messages":job.messages,"recipe_revision":job.descriptor.recipe.as_ref().map(|r|r.revision)}),
        ),
        "login" => {
            // Owned harmless test-session button; no credentials are inspected.
            page.find_element("#owned-login").await?.click().await?;
            Ok(json!({"state":"user_action_sent","authentication":"unverified"}))
        }
        "finish" => {
            job.readiness.transition_user_done();
            Ok(serde_json::to_value(&job.readiness)?)
        }
        "probe" => {
            let observed = page_value(
                page,
                "fetch('/ready',{cache:'no-store'}).then(r=>r.json()).then(v=>v.ready===true)"
                    .into(),
            )
            .await;
            let live = observed.is_ok();
            let probe = match observed {
                Ok(Value::Bool(true)) => ProbeOutcome::Ready,
                Ok(Value::Bool(false)) => ProbeOutcome::LoginRequired,
                _ => ProbeOutcome::Unprobeable("owned readiness endpoint unavailable".into()),
            };
            job.readiness.apply_probe(probe, live);
            Ok(serde_json::to_value(&job.readiness)?)
        }
        "send" => {
            if !job.readiness.is_ready() {
                bail!("session readiness unverified");
            }
            let turn = command.turn_id.ok_or_else(|| anyhow!("turn required"))?;
            let prompt = command.prompt.ok_or_else(|| anyhow!("prompt required"))?;
            if prompt.len() > 64 * 1024 || turn.is_empty() || turn.len() > 128 {
                bail!("message bounds exceeded");
            }
            if let Some((old, message)) = job.turns.get(&turn) {
                if old != &prompt {
                    bail!("turn payload conflict");
                }
                return message
                    .clone()
                    .map(|m| json!({"status":"completed","replayed":true,"message":m}))
                    .ok_or_else(|| anyhow!("submission_unknown; blind resend refused"));
            }
            if let Some(recipe) = &job.descriptor.recipe {
                recipe
                    .validate_replay(
                        &job.descriptor.components.origin,
                        &BTreeMap::from([("prompt".into(), prompt.clone())]),
                        &job.descriptor.components,
                    )
                    .map_err(|e| anyhow!("{e:?}"))?;
            }
            let selectors = job
                .descriptor
                .components
                .locators
                .iter()
                .map(|l| (role_key(&l.role), l.value.as_str()))
                .collect::<BTreeMap<_, _>>();
            let selectors_json = serde_json::to_string(&selectors)?;
            let valid = page_value(page, format!("(()=>{{const s={selectors_json};return ['composer','submit','generating','error'].every(k=>document.querySelectorAll(s[k]).length===1)}})()" )).await?;
            if valid != Value::Bool(true) {
                bail!("missing/ambiguous component; no submit");
            }
            let composer = serde_json::to_string(selectors["composer"])?;
            let insertion = format!("(()=>{{const e=document.querySelector({composer});e.value={};e.dispatchEvent(new Event('input',{{bubbles:true}}));return e.value}})()",serde_json::to_string(&prompt)?);
            if page_value(page, insertion).await? != Value::String(prompt.clone()) {
                bail!("prompt bytes changed");
            }
            // Bind the immutable turn before the irreversible click. Any error
            // after this point leaves unknown, never an automatic resend.
            job.turns.insert(turn.clone(), (prompt.clone(), None));
            let count_script = format!(
                "document.querySelectorAll({}).length",
                serde_json::to_string(selectors["assistant_messages"])?
            );
            let before = page_value(page, count_script).await?.as_u64().unwrap_or(0);
            page.find_element(selectors["submit"])
                .await?
                .click()
                .await?;
            job.messages += 1;
            for _ in 0..100 {
                let result = page_value(page, format!("(()=>{{const s={selectors_json};const a=[...document.querySelectorAll(s.assistant_messages)];const e=document.querySelector(s.error);if(e.textContent)return {{error:true}};if(a.length<={}||!document.querySelector(s.generating).hidden)return null;const m=a[a.length-1];return {{id:m.dataset.messageId,role:m.dataset.role,terminal:m.dataset.terminal==='true',content:m.textContent}}}})()",before)).await?;
                if result.get("error") == Some(&Value::Bool(true)) {
                    bail!("owned chat error; no resend");
                }
                if result.get("terminal") == Some(&Value::Bool(true))
                    && result.get("role").and_then(Value::as_str) == Some("assistant")
                {
                    let content = result["content"]
                        .as_str()
                        .ok_or_else(|| anyhow!("missing content"))?
                        .to_string();
                    if content.len() > 256 * 1024 {
                        bail!("assistant message exceeds limit; not adopted");
                    }
                    let message = WebchatMessage {
                        protocol_version: WB03_MESSAGE_PROTOCOL_V1.into(),
                        job_id: command.job_id.clone(),
                        lease_id: job.readiness.lease_id.clone(),
                        conversation_id: job.readiness.lease_id.clone(),
                        turn_id: turn.clone(),
                        message_id: result["id"]
                            .as_str()
                            .ok_or_else(|| anyhow!("missing message identity"))?
                            .into(),
                        role: "assistant".into(),
                        content,
                        terminal: true,
                        truncated: false,
                        truncation_reason: None,
                    };
                    job.turns.insert(turn, (prompt, Some(message.clone())));
                    return Ok(
                        json!({"status":"completed","replayed":false,"message":message,"browser_messages":job.messages}),
                    );
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            bail!("submission_unknown: terminal timeout; blind resend refused")
        }
        _ => bail!("unsupported owned command"),
    }
}
pub async fn serve(root: PathBuf, port: u16) -> Result<()> {
    std::fs::create_dir_all(&root)?;
    let descriptor: ProviderDescriptor =
        serde_json::from_slice(&std::fs::read(root.join("provider.json"))?)?;
    descriptor.validate()?;
    let shared = Arc::new(Mutex::new(Runtime {
        root,
        descriptor,
        job: None,
    }));
    let router = Router::new()
        .route("/descriptor", get(descriptor_handler))
        .route("/command", post(command))
        .with_state(shared);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("owned_webchat_address={}", listener.local_addr()?);
    axum::serve(listener, router).await?;
    Ok(())
}
async fn descriptor_handler(state: State<Shared>) -> Json<Value> {
    descriptor(state).await
}
