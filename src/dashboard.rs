use axum::http::header::CACHE_CONTROL;
use axum::http::HeaderValue;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;

pub(super) fn router() -> Router {
    Router::new()
        .route("/dashboard", get(account_dashboard))
        .route("/dashboard/", get(account_dashboard))
        .route("/projects/{namespace}/{slug}", get(project_workspace))
}

async fn account_dashboard() -> Response {
    private_page("Your projects", DASHBOARD_BODY)
}

async fn project_workspace() -> Response {
    private_page("Project", PROJECT_BODY)
}

fn document(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"theme-color\" content=\"#101415\"><title>{title} · Vapor</title>{STYLE}</head><body><main>{body}</main></body></html>"
    )
}

fn private_page(title: &str, body: &str) -> Response {
    let mut response = Html(document(title, body)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

const STYLE: &str = r#"<style>
:root{color-scheme:dark;font-family:Inter,ui-sans-serif,system-ui,sans-serif;background:#101415;color:#eef2ef;font-synthesis:none}*{box-sizing:border-box}body{margin:0;min-height:100vh;background:#101415}a{color:#a7d5c5}a:focus-visible,button:focus-visible{outline:2px solid #9fe1ce;outline-offset:3px}main{width:min(1280px,calc(100% - 48px));margin:0 auto;padding:24px 0 56px}.topbar{display:flex;align-items:center;gap:20px;min-height:40px}.brand{margin-right:auto;color:#eef2ef;font-weight:500;text-decoration:none}.topbar nav{display:flex;gap:16px}.topbar nav a{color:#b8c2bd;text-decoration:none;font-size:.9rem}.topbar nav a:hover{color:#fff}.eyebrow{color:#b6c2bb;font-size:.86rem;font-weight:500}h1{margin:8px 0 8px;font-size:clamp(1.8rem,3vw,2.4rem);font-weight:500;letter-spacing:-.025em;line-height:1.15}.lead{max-width:720px;margin:0;color:#bdc7c2;font-size:.98rem;line-height:1.55}.heading{padding:36px 0 20px}.panel{border:1px solid #303a34;border-radius:8px;background:#171c19}.project-list{display:grid;gap:8px;margin:16px 0 0;padding:0;list-style:none}.project-card{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 16px}.project-card a{text-decoration:none}.project-card a:hover .project-title{text-decoration:underline}.project-title{display:block;color:#eef2ef;font-size:1rem;font-weight:500}.project-summary{display:block;margin-top:4px;color:#b8c2bd;font-size:.88rem;line-height:1.45}.role{flex:none;color:#b8c2bd;font-size:.82rem}.state{margin:8px 0;padding:12px 16px;color:#c2c9c4;line-height:1.5}.state[hidden]{display:none}.state a{font-weight:500}.button{display:inline-flex;align-items:center;justify-content:center;min-height:36px;margin-top:10px;padding:0 12px;border:1px solid #9fe1ce;border-radius:6px;background:#9fe1ce;color:#13201b;font:inherit;font-size:.9rem;font-weight:500;cursor:pointer}.workspace-meta{display:flex;flex-wrap:wrap;gap:8px 20px;margin-top:12px;color:#b8c2bd;font-size:.88rem}.workspace-meta code{color:#d3ddd7}.back{display:inline-block;margin-top:16px;text-decoration:none}.error{border-color:#74434a;background:#251a1c}.empty{border-color:#49563c;background:#1b211b}@media(max-width:600px){main{width:calc(100% - 32px);padding-top:16px}.topbar{align-items:flex-start;flex-wrap:wrap}.topbar nav{width:100%;flex-wrap:wrap;justify-content:space-between;gap:8px}.heading{padding-top:32px}.project-card{align-items:flex-start}.project-summary{max-width:60ch}}
</style>"#;

const DASHBOARD_BODY: &str = r#"
<header class="topbar"><a class="brand" href="/">Vapor</a><nav aria-label="Main navigation"><a href="/downloads">Get Vapor</a><a href="/docs/">Documentation</a><a href="/operator/login">Operator access</a><a href="/logout">Sign out</a></nav></header>
<section class="heading"><span class="eyebrow">Your workspace</span><h1>Your projects</h1><p class="lead">Projects you can access, with their own content and tools.</p></section>
<section aria-live="polite" aria-busy="true"><p id="loading" class="panel state">Loading your projects…</p><div id="signed-out" class="panel state" hidden><strong>Sign in to continue.</strong><br><a href="/login">Sign in to Vapor</a></div><div id="empty" class="panel state empty" hidden><strong>No projects yet.</strong><br>When you’re added to a project, it will appear here.</div><div id="unavailable" class="panel state error" hidden><strong>Projects are temporarily unavailable.</strong><br>Try again in a moment. Your access has not changed.<br><button class="button" id="retry" type="button">Try again</button></div><ul id="projects" class="project-list" hidden></ul></section>
<script>
const stateIds=['loading','signed-out','empty','unavailable'];
function show(id){for(const item of stateIds)document.getElementById(item).hidden=item!==id;document.getElementById('projects').hidden=id!=='loaded';document.querySelector('section[aria-live]').setAttribute('aria-busy',id==='loading'?'true':'false');}
async function loadProjects(){show('loading');const list=document.getElementById('projects');list.replaceChildren();try{const response=await fetch('/api/registry/v1/me/projects',{credentials:'same-origin',cache:'no-store',headers:{'accept':'application/json'}});if(response.status===401||response.status===403){show('signed-out');return;}if(!response.ok)throw new Error('service');const data=await response.json();if(!Array.isArray(data.projects))throw new Error('response');if(data.projects.length===0){show('empty');return;}for(const project of data.projects){if(!project||typeof project.namespace!=='string'||typeof project.slug!=='string'||typeof project.display_name!=='string'||typeof project.role!=='string')continue;const item=document.createElement('li');item.className='panel project-card';const link=document.createElement('a');link.href='/projects/'+encodeURIComponent(project.namespace)+'/'+encodeURIComponent(project.slug);const title=document.createElement('span');title.className='project-title';title.textContent=project.display_name;const summary=document.createElement('span');summary.className='project-summary';summary.textContent=typeof project.summary==='string'?project.summary:'';link.append(title,summary);const role=document.createElement('span');role.className='role';role.textContent=project.role;item.append(link,role);list.append(item);}if(!list.children.length)throw new Error('response');show('loaded');}catch(_){show('unavailable');}}
document.getElementById('retry').addEventListener('click',loadProjects);loadProjects();
</script>
"#;

const PROJECT_BODY: &str = r#"
<header class="topbar"><a class="brand" href="/">Vapor</a><nav aria-label="Main navigation"><a href="/dashboard">Your projects</a><a href="/docs/">Documentation</a><a href="/operator/login">Operator access</a><a href="/logout">Sign out</a></nav></header>
<section class="heading"><span class="eyebrow">Project workspace</span><h1 id="title">Loading project…</h1><p id="summary" class="lead"></p></section>
<section aria-live="polite"><p id="loading" class="panel state">Checking project access…</p><div id="signed-out" class="panel state" hidden><strong>Sign in to continue.</strong><br><a href="/login">Sign in to Vapor</a></div><div id="unavailable" class="panel state error" hidden><strong>This project is unavailable.</strong><br>It may not exist, or your access may have changed.</div><div id="loaded" class="panel state" hidden><div class="workspace-meta"><span>Project ID: <code id="project-id"></code></span><span>Your access: <strong id="project-role"></strong></span><span>Status: <strong id="project-state"></strong></span></div><a class="back" href="/dashboard">← Back to your projects</a></div></section>
<script>
const parts=window.location.pathname.split('/').filter(Boolean);const namespace=parts.length===3?decodeURIComponent(parts[1]):'';const slug=parts.length===3?decodeURIComponent(parts[2]):'';const loading=document.getElementById('loading');const signedOut=document.getElementById('signed-out');const unavailable=document.getElementById('unavailable');const loaded=document.getElementById('loaded');
async function loadProject(){try{const url='/api/registry/v1/projects/'+encodeURIComponent(namespace)+'/'+encodeURIComponent(slug);const response=await fetch(url,{credentials:'same-origin',cache:'no-store',headers:{'accept':'application/json'}});loading.hidden=true;if(response.status===401){signedOut.hidden=false;return;}if(!response.ok){unavailable.hidden=false;return;}const data=await response.json();const project=data.project;const membership=data.membership;if(!project||!membership)throw new Error('response');document.title=String(project.display_name||'Project')+' · Vapor';document.getElementById('title').textContent=String(project.display_name||'Project');document.getElementById('summary').textContent=typeof project.summary==='string'?project.summary:'';document.getElementById('project-id').textContent=String(project.id||'');document.getElementById('project-role').textContent=String(membership.role||'');document.getElementById('project-state').textContent=String(project.state||'');loaded.hidden=false;}catch(_){loading.hidden=true;unavailable.hidden=false;}}
loadProject();
</script>
"#;

#[cfg(test)]
mod tests {
    use super::router;
    use axum::body::{to_bytes, Body};
    use axum::http::{header::CACHE_CONTROL, Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn dashboard_is_a_user_view_without_operator_navigation() {
        let response = router()
            .oneshot(Request::get("/dashboard").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("Your projects"));
        assert!(html.contains("/api/registry/v1/me/projects"));
        assert!(html.contains("textContent"));
        assert!(!html.contains("/admin"));
        assert!(!html.contains("radial-gradient"));
        assert!(!html.contains("text-transform:uppercase"));
    }

    #[tokio::test]
    async fn project_workspace_uses_the_membership_gated_registry_api() {
        let response = router()
            .oneshot(
                Request::get("/projects/ghf-studios/loo-cast")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("/api/registry/v1/projects/"));
        assert!(html.contains("Project workspace"));
        assert!(!html.contains("/admin"));
    }
}
