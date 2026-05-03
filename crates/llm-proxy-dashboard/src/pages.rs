use std::fmt::Write;

use axum::response::{Html, IntoResponse, Response};

use crate::{
    render::{escape_html, internal_error, page},
    DashboardState,
};

pub(crate) fn dashboard_page(state: &DashboardState) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>LLM Proxy</title>
</head>
<body>
  <main>
    <h1>LLM Proxy</h1>
    <p>Dashboard scaffold is running.</p>
    <dl>
      <dt>Proxy listen</dt><dd>{}</dd>
      <dt>Admin listen</dt><dd>{}</dd>
      <dt>Default route</dt><dd>{}</dd>
    </dl>
    <nav>
      <a href="/keys">Proxy API keys</a>
      <a href="/upstream-secrets">Upstream secrets</a>
      <form method="post" action="/logout"><button type="submit">Log out</button></form>
    </nav>
  </main>
</body>
</html>"#,
        state.config.proxy_listen, state.config.admin_listen, state.config.default_route
    ))
}

pub(crate) async fn render_keys_page(state: &DashboardState, new_key: Option<String>) -> Response {
    let Ok(keys) = state.database.list_proxy_api_keys().await else {
        return internal_error();
    };

    let mut body = String::new();
    body.push_str("<h1>Proxy API keys</h1>");
    if let Some(token) = new_key {
        let _ = write!(
            body,
            "<p>New key. This is shown once:</p><pre>{}</pre>",
            escape_html(&token)
        );
    }
    body.push_str(
        r#"
<form method="post" action="/keys">
  <label>Label <input name="label" required></label>
  <button type="submit">Create key</button>
</form>
<table>
  <thead><tr><th>Label</th><th>Created</th></tr></thead>
  <tbody>
"#,
    );
    for key in keys {
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(&key.label),
            escape_html(&key.created_at)
        );
    }
    body.push_str("</tbody></table><p><a href=\"/\">Dashboard</a></p>");

    Html(page("API Keys", &body)).into_response()
}

pub(crate) async fn render_upstream_secrets_page(
    state: &DashboardState,
    message: Option<&str>,
) -> Response {
    let Ok(secrets) = state.database.list_upstream_secrets().await else {
        return internal_error();
    };

    let mut body = String::new();
    body.push_str("<h1>Upstream secrets</h1>");
    if let Some(message) = message {
        let _ = write!(body, "<p>{}</p>", escape_html(message));
    }
    body.push_str(
        r#"
<form method="post" action="/upstream-secrets">
  <label>Name <input name="name" required></label>
  <label>API key <input name="value" type="password" required></label>
  <button type="submit">Save secret</button>
</form>
<table>
  <thead><tr><th>Name</th><th>Updated</th><th></th></tr></thead>
  <tbody>
"#,
    );
    for secret in secrets {
        let _ = write!(
            body,
            r#"<tr>
<td>{}</td>
<td>{}</td>
<td>
  <form method="post" action="/upstream-secrets/delete">
    <input type="hidden" name="name" value="{}">
    <button type="submit">Delete</button>
  </form>
</td>
</tr>"#,
            escape_html(&secret.name),
            escape_html(&secret.updated_at),
            escape_html(&secret.name)
        );
    }
    body.push_str("</tbody></table><p><a href=\"/\">Dashboard</a></p>");

    Html(page("Upstream Secrets", &body)).into_response()
}
