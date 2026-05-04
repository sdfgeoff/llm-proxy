use std::fmt::Write;

use llm_proxy_db::{DashboardMetrics, DimensionMetric, HourlyMetric, StatusMetric};

use crate::render::escape_html;

pub(crate) fn render_hourly_charts(body: &mut String, hourly: &[HourlyMetric]) {
    body.push_str("<section><h2>Traffic over time</h2>");
    if hourly.is_empty() {
        body.push_str("<p>No requests recorded in the selected period.</p></section>");
        return;
    }
    body.push_str("<h3>Requests</h3>");
    render_line_chart(
        body,
        hourly.iter().map(|point| point.request_count as f64),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("<h3>Tokens</h3>");
    render_line_chart(
        body,
        hourly.iter().map(|point| point.total_tokens as f64),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("<h3>Tokens/sec</h3>");
    render_line_chart(
        body,
        hourly
            .iter()
            .map(|point| point.avg_tokens_per_second.unwrap_or(0.0)),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("<h3>Time to first token</h3>");
    render_line_chart(
        body,
        hourly
            .iter()
            .map(|point| point.avg_time_to_first_token_ms.unwrap_or(0.0)),
        |idx| hourly[idx].bucket.clone(),
    );
    body.push_str("</section>");
}

pub(crate) fn render_dimension_table(
    body: &mut String,
    title: &str,
    label_heading: &str,
    rows: &[DimensionMetric],
) {
    let chart_title = format!("{title} token volume");
    render_bar_chart(body, &chart_title, rows, |row| row.total_tokens as f64);

    let max_tokens = rows
        .iter()
        .map(|row| row.total_tokens)
        .max()
        .unwrap_or_default()
        .max(1);
    let _ = write!(
        body,
        "<section><h2>{}</h2><table><thead><tr>\
         <th>{}</th><th>Requests</th><th>Tokens</th><th>Tokens/sec</th><th>TTFT</th><th></th>\
         </tr></thead><tbody>",
        escape_html(title),
        escape_html(label_heading)
    );
    for row in rows {
        let width = (row.total_tokens * 100 / max_tokens).max(1);
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
             <td><div style=\"background:#2563eb;height:0.75rem;width:{}%;\"></div></td></tr>",
            escape_html(&row.label),
            row.request_count,
            row.total_tokens,
            format_float(row.avg_tokens_per_second),
            format_float_ms(row.avg_time_to_first_token_ms),
            width
        );
    }
    body.push_str("</tbody></table></section>");
}

pub(crate) fn render_status_table(body: &mut String, metrics: &DashboardMetrics) {
    render_status_chart(body, &metrics.by_status);
    body.push_str(
        "<section><h2>Status rates</h2><table><thead><tr><th>Status</th><th>Requests</th></tr></thead><tbody>",
    );
    for row in &metrics.by_status {
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(&row.label),
            row.request_count
        );
    }
    body.push_str("</tbody></table></section>");
}

fn render_line_chart<I, F>(body: &mut String, values: I, label: F)
where
    I: IntoIterator<Item = f64>,
    F: Fn(usize) -> String,
{
    let values: Vec<f64> = values.into_iter().collect();
    let max = values.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    let width = 640.0;
    let height = 160.0;
    let step = if values.len() > 1 {
        width / (values.len() - 1) as f64
    } else {
        width
    };
    let points = values
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            let x = idx as f64 * step;
            let y = height - (value / max * height);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let first = label(0);
    let last = label(values.len().saturating_sub(1));
    let _ = write!(
        body,
        "<svg viewBox=\"0 0 700 210\" role=\"img\" style=\"width:100%;max-width:720px;height:auto;\">\
         <polyline fill=\"none\" stroke=\"#2563eb\" stroke-width=\"3\" points=\"{}\" transform=\"translate(30 20)\"/>\
         <text x=\"30\" y=\"200\">{}</text><text x=\"560\" y=\"200\">{}</text>\
         <text x=\"30\" y=\"18\">max {}</text></svg>",
        points,
        escape_html(&first),
        escape_html(&last),
        format_float(Some(max))
    );
}

fn render_bar_chart(
    body: &mut String,
    title: &str,
    rows: &[DimensionMetric],
    value: impl Fn(&DimensionMetric) -> f64,
) {
    if rows.is_empty() {
        return;
    }
    let max = rows.iter().map(&value).fold(0.0_f64, f64::max).max(1.0);
    let bar_height = 22.0;
    let gap = 10.0;
    let width = 680.0;
    let height = rows.len() as f64 * (bar_height + gap) + 30.0;
    let _ = write!(
        body,
        "<section><h3>{}</h3><svg viewBox=\"0 0 720 {:.0}\" role=\"img\" style=\"width:100%;max-width:720px;height:auto;\">",
        escape_html(title),
        height
    );
    for (idx, row) in rows.iter().enumerate() {
        let y = idx as f64 * (bar_height + gap) + 10.0;
        let bar_width = value(row) / max * width;
        let _ = write!(
            body,
            "<text x=\"0\" y=\"{:.1}\" dominant-baseline=\"hanging\">{}</text>\
             <rect x=\"180\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#2563eb\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" dominant-baseline=\"hanging\">{}</text>",
            y,
            escape_html(&row.label),
            y,
            bar_width.max(1.0),
            bar_height,
            188.0 + bar_width.max(1.0),
            y,
            row.total_tokens
        );
    }
    body.push_str("</svg></section>");
}

fn render_status_chart(body: &mut String, rows: &[StatusMetric]) {
    if rows.is_empty() {
        return;
    }
    let max = rows
        .iter()
        .map(|row| row.request_count as f64)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let bar_width = 88.0;
    let gap = 20.0;
    let height = 180.0;
    body.push_str(
        "<section><h3>Status distribution</h3><svg viewBox=\"0 0 720 220\" role=\"img\" style=\"width:100%;max-width:720px;height:auto;\">",
    );
    for (idx, row) in rows.iter().enumerate() {
        let x = 40.0 + idx as f64 * (bar_width + gap);
        let value_height = row.request_count as f64 / max * height;
        let y = 190.0 - value_height;
        let _ = write!(
            body,
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#0f766e\"/>\
             <text x=\"{:.1}\" y=\"205\" text-anchor=\"middle\">{}</text>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
            x,
            y,
            bar_width,
            value_height.max(1.0),
            x + bar_width / 2.0,
            escape_html(&row.label),
            x + bar_width / 2.0,
            (y - 6.0).max(10.0),
            row.request_count
        );
    }
    body.push_str("</svg></section>");
}

fn format_float(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "-".to_owned())
}

fn format_float_ms(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0} ms"))
        .unwrap_or_else(|| "-".to_owned())
}
