use std::fmt::Write;

use axum::response::{Html, IntoResponse, Response};
use llm_proxy_db::{RequestDetail, RequestSummary};

use crate::{
    render::{escape_html, internal_error, page},
    DashboardState,
};

pub(crate) async fn render_requests_page(state: &DashboardState) -> Response {
    let Ok(requests) = state.database.recent_requests(100).await else {
        return internal_error();
    };

    let mut body = String::new();
    body.push_str("<h1>Requests</h1>");
    body.push_str(
        r#"<table>
<thead>
<tr><th>Started</th><th>Key</th><th>Endpoint</th><th>Model</th><th>Route</th><th>Status</th><th>Duration</th><th>Tokens</th><th>Payload</th></tr>
</thead>
<tbody>"#,
    );
    for request in requests {
        write_request_row(&mut body, &request);
    }
    body.push_str("</tbody></table><p><a href=\"/\">Dashboard</a></p>");
    Html(page("Requests", &body)).into_response()
}

pub(crate) fn render_request_detail_page(request: &RequestDetail) -> Response {
    let mut body = String::new();
    let _ = write!(
        body,
        "<h1>Request {}</h1><dl>\
         <dt>Started</dt><dd>{}</dd>\
         <dt>API key</dt><dd>{}</dd>\
         <dt>Endpoint</dt><dd>{}</dd>\
         <dt>Requested model</dt><dd>{}</dd>\
         <dt>Upstream model</dt><dd>{}</dd>\
         <dt>Route</dt><dd>{}</dd>\
         <dt>Status</dt><dd>{}</dd>\
         <dt>Duration</dt><dd>{}</dd>\
         <dt>Token source</dt><dd>{}</dd>\
         <dt>Input tokens</dt><dd>{}</dd>\
         <dt>Output tokens</dt><dd>{}</dd>\
         <dt>Total tokens</dt><dd>{}</dd>\
         <dt>Payload capture</dt><dd>{}</dd>\
         </dl>",
        escape_html(&request.id),
        escape_html(&request.started_at),
        escape_option(request.proxy_key_label.as_deref()),
        escape_html(&request.endpoint),
        escape_option(request.requested_model.as_deref()),
        escape_option(request.upstream_model.as_deref()),
        escape_option(request.route_name.as_deref()),
        format_option_i64(request.http_status),
        format_duration(request.duration_ms),
        escape_option(request.token_source.as_deref()),
        format_option_i64(request.input_tokens),
        format_option_i64(request.output_tokens),
        format_option_i64(request.total_tokens),
        escape_html(&request.payload_capture_status),
    );

    body.push_str("<h2>Payloads</h2><ul>");
    payload_link(&mut body, request, "request", request.request_payload_bytes);
    payload_link(
        &mut body,
        request,
        "response",
        request.response_payload_bytes,
    );
    body.push_str("</ul>");

    if let Some(error) = &request.payload_capture_error {
        let _ = write!(body, "<p>Capture error: {}</p>", escape_html(error));
    }
    if let Some(usage) = &request.provider_usage_json {
        let _ = write!(
            body,
            "<h2>Provider usage</h2><pre>{}</pre>",
            escape_html(usage)
        );
    }
    body.push_str("<p><a href=\"/requests\">Requests</a></p>");

    Html(page("Request Detail", &body)).into_response()
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

fn write_request_row(body: &mut String, request: &RequestSummary) {
    let _ = write!(
        body,
        "<tr><td><a href=\"/requests/{}\">{}</a></td>\
         <td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
         <td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
        escape_html(&request.id),
        escape_html(&request.started_at),
        escape_option(request.proxy_key_label.as_deref()),
        escape_html(&request.endpoint),
        escape_option(request.requested_model.as_deref()),
        escape_option(request.route_name.as_deref()),
        format_option_i64(request.http_status),
        format_duration(request.duration_ms),
        format_option_i64(request.total_tokens),
        escape_html(&request.payload_capture_status),
    );
}

fn payload_link(body: &mut String, request: &RequestDetail, kind: &str, bytes: Option<i64>) {
    let has_payload = match kind {
        "request" => request.request_payload_path.is_some(),
        "response" => request.response_payload_path.is_some(),
        _ => false,
    };
    if has_payload {
        let _ = write!(
            body,
            "<li><a href=\"/requests/{}/payload/{}\">{} payload</a> ({})</li>",
            escape_html(&request.id),
            kind,
            kind,
            format_bytes(bytes),
        );
    } else {
        let _ = write!(body, "<li>{kind} payload unavailable</li>");
    }
}

fn escape_option(value: Option<&str>) -> String {
    value.map(escape_html).unwrap_or_else(|| "-".to_owned())
}

fn format_option_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn format_duration(value: Option<i64>) -> String {
    value
        .map(|value| format!("{value} ms"))
        .unwrap_or_else(|| "-".to_owned())
}

fn format_bytes(value: Option<i64>) -> String {
    value
        .map(|value| format!("{value} bytes"))
        .unwrap_or_else(|| "unknown size".to_owned())
}
