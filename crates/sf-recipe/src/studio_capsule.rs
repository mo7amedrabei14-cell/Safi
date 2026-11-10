//! Offline interactive capsule export (`STUDIO-PHASE-4`, section 6.3).
//!
//! The exporter produces one `.safi.html` file that opens from `file://` with the network fully
//! disabled. It contains **no JavaScript at all**: slides and filters are implemented with CSS
//! only, so there is no script execution surface for untrusted text and no CDN, font, or API
//! dependency.
//!
//! Every number in the file was computed by Rust at export time and carries its metric name,
//! formula, formula version, dataset version, and `BLAKE3` fingerprint. The file states plainly
//! that it is a snapshot: editing it never flows back into the Safi project, and a filtered view
//! only selects between aggregates Rust already computed.

use rust_decimal::Decimal;
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};

use crate::datagraph::{AggFunc, DataGraph, datagraph_display};

/// Maximum accepted size of the generated document (8 MiB).
pub const MAX_CAPSULE_BYTES: usize = 8 * 1024 * 1024;

/// A chart the caller wants inside the capsule. Aggregation happens in Rust, not in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleChart {
    /// Chart title (already localized by the caller).
    pub title: String,
    /// Aggregation function.
    pub function: AggFunc,
    /// Column being aggregated.
    pub column: String,
    /// Column used for grouping.
    pub group_by: String,
}

/// Options for [`export_capsule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleOptions {
    /// Project title shown in the header.
    pub title: String,
    /// ISO-8601 UTC creation timestamp (supplied by the caller, never invented here).
    pub created_at: String,
    /// Dashboard charts.
    pub charts: Vec<CapsuleChart>,
}

fn export_failed() -> SafiError {
    SafiError::new(
        ErrorCode::SF_SYS_001,
        ErrorSeverity::Error,
        "CapsuleExportFailed",
        "studio.capsule_failed",
    )
}

/// Escapes text so it can only ever be data inside HTML.
#[must_use]
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn bar_width(value: Decimal, maximum: Decimal) -> u32 {
    if maximum <= Decimal::ZERO || value <= Decimal::ZERO {
        return 0;
    }
    let ratio = value.checked_div(maximum).unwrap_or(Decimal::ZERO);
    let percent = (ratio * Decimal::from(100)).round();
    let clamped = percent.max(Decimal::ZERO).min(Decimal::from(100));
    u32::try_from(clamped.trunc()).unwrap_or(100)
}

const STYLE: &str = r#"
:root{color-scheme:light dark}
*{box-sizing:border-box}
body{margin:0;font-family:system-ui,"Segoe UI",Tahoma,"Noto Naskh Arabic",sans-serif;line-height:1.6;background:#0b1220;color:#e8eefc}
main{max-width:1100px;margin:0 auto;padding:32px 20px 64px}
h1{font-size:28px;margin:0 0 8px}
h2{font-size:20px;margin:32px 0 12px}
.badge{display:inline-block;padding:2px 10px;border-radius:999px;background:#1d4ed8;color:#fff;font-size:12px}
.meta{color:#9fb0c9;font-size:13px;margin:8px 0 0}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px}
.card{background:#121b2d;border:1px solid #1f2b42;border-radius:14px;padding:16px}
.card h3{margin:0 0 8px;font-size:16px}
.value{font-size:26px;font-weight:700;margin:4px 0}
.provenance{color:#8fa3c0;font-size:12px;word-break:break-all}
table{width:100%;border-collapse:collapse;font-size:13px}
th,td{text-align:start;padding:8px;border-bottom:1px solid #1f2b42;vertical-align:top}
.bars{display:grid;gap:8px}
.bar{display:flex;align-items:center;gap:10px}
.bar span{min-width:90px}
.track{flex:1;background:#1b2740;border-radius:999px;height:12px;overflow:hidden}
.fill{height:100%;background:linear-gradient(90deg,#2563eb,#22d3ee)}
.filters{display:flex;flex-wrap:wrap;gap:8px;margin:8px 0 12px}
.filters input{position:absolute;opacity:0;pointer-events:none}
.filters label{cursor:pointer;padding:4px 10px;border-radius:999px;border:1px solid #2b3b58;font-size:12px}
@media print{.filters{display:none}}
"#;

fn filter_rules(chart_index: usize, groups: &[crate::datagraph::GroupedValue]) -> String {
    let mut rules = String::new();
    for (group_index, _) in groups.iter().enumerate() {
        rules.push_str(&format!(
            "#c{chart_index}-{group_index}:checked ~ .bars .bar{{display:none}}"
        ));
        rules.push_str(&format!(
            "#c{chart_index}-{group_index}:checked ~ .bars .bar.g{group_index}{{display:flex}}"
        ));
    }
    rules
}

/// Renders the self-contained capsule document.
///
/// # Errors
/// `SF-SYS-001` when a chart references an unknown column or the document exceeds
/// [`MAX_CAPSULE_BYTES`].
pub fn export_capsule(graph: &DataGraph, options: &CapsuleOptions) -> Result<String, SafiError> {
    let mut html = String::with_capacity(16 * 1024);
    html.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\" dir=\"auto\">\n<head>\n<meta charset=\"utf-8\">\n",
    );
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    // No scripts exist in this document, so script-src can stay 'none'.
    html.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\">\n",
    );
    html.push_str(&format!(
        "<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n",
        escape_html(&options.title)
    ));
    html.push_str(&format!(
        "<h1>{}</h1>\n<p><span class=\"badge\">snapshot</span></p>\n",
        escape_html(&options.title)
    ));
    html.push_str(&format!(
        "<p class=\"meta\">Created: {} · Dataset version: {} · Revision: {} · Source: {}</p>\n",
        escape_html(&options.created_at),
        graph.dataset_version().get(),
        graph.revision().get(),
        escape_html(graph.source_id().as_str()),
    ));
    html.push_str(
        "<p class=\"meta\">This file is a fixed snapshot computed by Safi. It is not a live connection: edits made here never return to the project, and every number below is bound to the fingerprint stored next to it.</p>\n",
    );

    html.push_str("<h2 id=\"dashboard\">Dashboard</h2>\n<div class=\"grid\">\n");
    let mut dynamic_css = String::new();
    for (chart_index, chart) in options.charts.iter().enumerate() {
        let groups = graph.grouped_aggregate(chart.function, &chart.column, &chart.group_by)?;
        let maximum = groups
            .iter()
            .map(|group| group.value)
            .fold(Decimal::ZERO, Decimal::max);
        html.push_str("<section class=\"card chart\">\n");
        html.push_str(&format!("<h3>{}</h3>\n", escape_html(&chart.title)));
        if groups.len() > 1 {
            html.push_str("<div class=\"filters\">\n");
            html.push_str(&format!("<label for=\"c{chart_index}-all\">All</label>\n"));
            for (group_index, group) in groups.iter().enumerate() {
                html.push_str(&format!(
                    "<label for=\"c{chart_index}-{group_index}\">{}</label>\n",
                    escape_html(&group.label)
                ));
            }
            html.push_str("</div>\n");
            html.push_str(&format!(
                "<input type=\"radio\" name=\"f{chart_index}\" id=\"c{chart_index}-all\" checked>\n"
            ));
            for (group_index, _) in groups.iter().enumerate() {
                html.push_str(&format!(
                    "<input type=\"radio\" name=\"f{chart_index}\" id=\"c{chart_index}-{group_index}\">\n"
                ));
            }
            dynamic_css.push_str(&filter_rules(chart_index, &groups));
        }
        html.push_str("<div class=\"bars\">\n");
        for (group_index, group) in groups.iter().enumerate() {
            html.push_str(&format!(
                "<div class=\"bar g{group_index}\"><span>{}</span><div class=\"track\"><div class=\"fill\" style=\"width:{}%\"></div></div><span class=\"provenance\">{}</span></div>\n",
                escape_html(&group.label),
                bar_width(group.value, maximum),
                escape_html(&datagraph_display(&sf_core::value::Value::Decimal(group.value))),
            ));
        }
        html.push_str("</div>\n");
        html.push_str(&format!(
            "<p class=\"provenance\">{} of {} grouped by {}</p>\n",
            escape_html(chart.function.as_str()),
            escape_html(&chart.column),
            escape_html(&chart.group_by)
        ));
        html.push_str("</section>\n");
    }
    html.push_str("</div>\n");
    if !dynamic_css.is_empty() {
        html.push_str(&format!("<style>{dynamic_css}</style>\n"));
    }

    html.push_str("<h2 id=\"slides\">Slides</h2>\n<div class=\"grid\">\n");
    for id in graph.metric_ids() {
        let Some(token) = graph.token(&id) else {
            continue;
        };
        html.push_str("<section class=\"card\">\n");
        html.push_str(&format!("<h3>{}</h3>\n", escape_html(&token.metric_name)));
        html.push_str(&format!(
            "<p class=\"value\">{} {}</p>\n",
            escape_html(&datagraph_display(&token.value)),
            escape_html(token.unit.as_deref().unwrap_or(""))
        ));
        html.push_str(&format!(
            "<p class=\"provenance\">formula: {} · v{} · data v{} · {}…</p>\n",
            escape_html(&token.formula),
            token.formula_version,
            token.source_version.get(),
            escape_html(&token.fingerprint()[..token.fingerprint().len().min(16)]),
        ));
        html.push_str("</section>\n");
    }
    html.push_str("</div>\n");

    html.push_str("<h2 id=\"report\">Verified metrics</h2>\n<table>\n<thead><tr><th>Metric</th><th>Value</th><th>Formula</th><th>Version</th><th>Fingerprint</th></tr></thead>\n<tbody>\n");
    for id in graph.metric_ids() {
        let Some(token) = graph.token(&id) else {
            continue;
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{} {}</td><td>{}</td><td>v{} / data v{}</td><td class=\"provenance\">{}</td></tr>\n",
            escape_html(&token.metric_name),
            escape_html(&datagraph_display(&token.value)),
            escape_html(token.unit.as_deref().unwrap_or("")),
            escape_html(&token.formula),
            token.formula_version,
            token.source_version.get(),
            escape_html(token.fingerprint()),
        ));
    }
    html.push_str("</tbody>\n</table>\n");
    html.push_str("<p class=\"meta\">Filter controls only select between aggregates computed by Safi before export; they never calculate a new number.</p>\n");
    html.push_str("</main>\n</body>\n</html>\n");

    if html.len() > MAX_CAPSULE_BYTES {
        return Err(export_failed());
    }
    Ok(html)
}

#[cfg(test)]
mod tests {
    use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
    use sf_core::datagraph::{MetricId, SourceId};
    use sf_core::error::SafiError;
    use sf_core::table::Table;
    use sf_core::value::CellValue;

    use super::{CapsuleChart, CapsuleOptions, escape_html, export_capsule};
    use crate::datagraph::{AggFunc, DataGraph};

    fn graph() -> Result<DataGraph, SafiError> {
        let columns = [
            ("region", ["Cairo", "Giza", "Cairo"].as_slice()),
            ("amount", ["100", "200", "50"].as_slice()),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            Column::new(
                ColumnSchema::new(index, name, ColumnType::Unknown, ConfidenceScore::ONE),
                values
                    .iter()
                    .map(|value| CellValue::from_raw(*value))
                    .collect(),
            )
        })
        .collect();
        let table = Table::new("tbl_sales", "sales", columns)?;
        let mut graph = DataGraph::new(SourceId::new("sales")?, table, 10)?;
        graph.define_metric(
            MetricId::new("total")?,
            "Total",
            Some("EGP"),
            "SUM(amount)",
            10,
        )?;
        Ok(graph)
    }

    fn options(title: &str) -> CapsuleOptions {
        CapsuleOptions {
            title: title.to_owned(),
            created_at: "2026-11-11T00:00:00Z".to_owned(),
            charts: vec![CapsuleChart {
                title: "Revenue by region".to_owned(),
                function: AggFunc::Sum,
                column: "amount".to_owned(),
                group_by: "region".to_owned(),
            }],
        }
    }

    #[test]
    fn the_capsule_carries_verified_values_and_their_provenance() -> Result<(), SafiError> {
        let graph = graph()?;
        let html = export_capsule(&graph, &options("Board review"))?;
        let token = graph.token(&MetricId::new("total")?).ok_or_else(|| {
            SafiError::new(
                sf_core::error::ErrorCode::SF_CAL_006,
                sf_core::error::ErrorSeverity::Error,
                "UnknownMetric",
                "studio.unknown_metric",
            )
        })?;
        assert!(html.contains("350"));
        assert!(html.contains(token.fingerprint()));
        assert!(html.contains("Dataset version: 0"));
        assert!(html.contains("SUM(amount)"));
        assert!(html.contains("snapshot"));
        assert!(html.contains("Cairo"));
        Ok(())
    }

    #[test]
    fn the_capsule_contains_no_script_surface_or_external_resource() -> Result<(), SafiError> {
        let graph = graph()?;
        let html = export_capsule(&graph, &options("<script>alert(1)</script>"))?;
        assert!(!html.contains("<script"));
        assert!(!html.to_lowercase().contains("javascript:"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("script-src") || html.contains("default-src 'none'"));
        Ok(())
    }

    #[test]
    fn hostile_text_is_escaped_in_every_position() -> Result<(), SafiError> {
        assert_eq!(
            escape_html("<img src=x onerror=alert(1)>"),
            "&lt;img src=x onerror=alert(1)&gt;"
        );
        let graph = graph()?;
        let mut options = options("ok");
        options.charts[0].title = "<b>bold</b>".to_owned();
        let html = export_capsule(&graph, &options)?;
        assert!(html.contains("&lt;b&gt;bold&lt;/b&gt;"));
        assert!(!html.contains("<b>bold</b>"));
        Ok(())
    }
}
