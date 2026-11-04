//! A widget's miniature data, taken from its visual's first render.
//!
//! The desktop lays visuals out natively, so the engine can't take a picture
//! of one. It can read what the visual shows: the element tree is semantic
//! (figures, charts, tables, text), and a miniature drawn from it stays
//! legible at card size where a scaled-down picture wouldn't. Taken whenever
//! content is published, so a refresh updates it with the new data.

use zeron_proto::{
    PROJECT_WIDGET_PREVIEW_POINTS, PROJECT_WIDGET_PREVIEW_SCALE, ProjectWidgetPreview,
    ProjectWidgetPreviewChart, ProjectWidgetPreviewChartKind, ProjectWidgetPreviewSeries,
    ProjectWidgetPreviewStat, ProjectWidgetPreviewTable,
};
use zeron_visual::{Chart, ChartKind, Node, NodeKind};

const MAX_STATS: usize = 3;
const MAX_SERIES: usize = 3;
const MAX_COLUMNS: usize = 3;
const MAX_ROWS: usize = 4;
/// Longest text kept for a figure, label or cell.
const MAX_FIELD: usize = 48;
const MAX_TEXT: usize = 160;

/// Render `script` once with `seed` and summarize what it shows. `None` when
/// the script fails; empty when it shows nothing a miniature can draw.
pub(super) fn extract(
    script: &str,
    seed: u64,
    assets: zeron_visual::Assets,
) -> Option<ProjectWidgetPreview> {
    let tree = zeron_visual::Visual::load(script, seed)
        .ok()?
        .with_assets(assets)
        .render()
        .ok()?;
    let mut preview = ProjectWidgetPreview::default();
    let mut text = None;
    walk(&tree, &mut preview, &mut text);
    if preview.stats.is_empty() && preview.chart.is_none() && preview.table.is_none() {
        preview.text = text;
    }
    Some(preview)
}

fn walk(node: &Node, preview: &mut ProjectWidgetPreview, text: &mut Option<String>) {
    match &node.kind {
        NodeKind::Stack(stack) => {
            for child in &stack.children {
                walk(child, preview, text);
            }
        }
        NodeKind::Grid { children, .. } => {
            for child in children {
                walk(child, preview, text);
            }
        }
        NodeKind::Stat {
            value,
            label,
            delta,
            ..
        } if preview.stats.len() < MAX_STATS => {
            preview.stats.push(ProjectWidgetPreviewStat {
                value: clip(value, MAX_FIELD),
                label: clip(label, MAX_FIELD),
                delta: delta.as_deref().map(|d| clip(d, MAX_FIELD)),
            });
        }
        NodeKind::Chart(chart) if preview.chart.is_none() => {
            preview.chart = summarize_chart(chart);
        }
        NodeKind::Table(table) if preview.table.is_none() => {
            let columns: Vec<String> = table
                .columns
                .iter()
                .take(MAX_COLUMNS)
                .map(|c| clip(&c.label, MAX_FIELD))
                .collect();
            let rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .take(MAX_ROWS)
                .map(|row| {
                    row.iter()
                        .take(MAX_COLUMNS)
                        .map(|cell| clip(&plain_text(cell), MAX_FIELD))
                        .collect()
                })
                .collect();
            if !rows.is_empty() {
                preview.table = Some(ProjectWidgetPreviewTable { columns, rows });
            }
        }
        NodeKind::Text(_) if text.is_none() => {
            let plain = plain_text(node);
            if !plain.trim().is_empty() {
                *text = Some(clip(plain.trim(), MAX_TEXT));
            }
        }
        NodeKind::Canvas(_) | NodeKind::Scene(_) | NodeKind::Shader(_) => preview.graphic = true,
        _ => {}
    }
}

fn plain_text(node: &Node) -> String {
    match &node.kind {
        NodeKind::Text(text) => text.spans.iter().map(|s| s.text.as_str()).collect(),
        NodeKind::Stat { value, .. } => value.clone(),
        NodeKind::Link { label, .. } => label.clone(),
        NodeKind::Stack(stack) => stack
            .children
            .iter()
            .map(plain_text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn summarize_chart(chart: &Chart) -> Option<ProjectWidgetPreviewChart> {
    let kind = match chart.kind {
        ChartKind::Bar => ProjectWidgetPreviewChartKind::Bar,
        ChartKind::Line => ProjectWidgetPreviewChartKind::Line,
        ChartKind::Area => ProjectWidgetPreviewChartKind::Area,
        ChartKind::Scatter => ProjectWidgetPreviewChartKind::Scatter,
    };
    let series: Vec<(String, Vec<Option<f64>>)> = chart
        .series
        .iter()
        .take(MAX_SERIES)
        .map(|series| {
            let values: Vec<Option<f64>> = if chart.kind == ChartKind::Scatter {
                // Points in x order read as the cloud's trend.
                let mut points: Vec<(f64, f64)> = series
                    .points
                    .iter()
                    .copied()
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .collect();
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                points.into_iter().map(|(_, y)| Some(y)).collect()
            } else {
                series
                    .values
                    .iter()
                    .map(|v| v.filter(|v| v.is_finite()))
                    .collect()
            };
            (series.name.clone(), downsample(&values))
        })
        .filter(|(_, values)| values.iter().any(Option::is_some))
        .collect();
    if series.is_empty() {
        return None;
    }
    // One scale for every series: stacked bars add up, so their tops do.
    let stacked = chart.stacked && chart.kind != ChartKind::Scatter;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    if stacked {
        let len = series.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        for ix in 0..len {
            let total: f64 = series
                .iter()
                .filter_map(|(_, v)| v.get(ix).copied().flatten())
                .sum();
            lo = lo.min(total.min(0.0));
            hi = hi.max(total);
        }
    } else {
        for value in series.iter().flat_map(|(_, v)| v.iter().flatten()) {
            lo = lo.min(*value);
            hi = hi.max(*value);
        }
    }
    // Bars and areas rise from zero when the data allows it.
    if matches!(chart.kind, ChartKind::Bar | ChartKind::Area) && lo > 0.0 {
        lo = 0.0;
    }
    let lo = chart.y_min.filter(|v| v.is_finite()).unwrap_or(lo);
    let hi = chart.y_max.filter(|v| v.is_finite()).unwrap_or(hi);
    let span = if hi > lo { hi - lo } else { 1.0 };
    let scale = f64::from(PROJECT_WIDGET_PREVIEW_SCALE);
    Some(ProjectWidgetPreviewChart {
        kind,
        stacked,
        series: series
            .into_iter()
            .map(|(name, values)| ProjectWidgetPreviewSeries {
                name: clip(&name, MAX_FIELD),
                values: values
                    .into_iter()
                    .map(|v| v.map(|v| (((v - lo) / span).clamp(0.0, 1.0) * scale).round() as u16))
                    .collect(),
            })
            .collect(),
    })
}

/// At most [`PROJECT_WIDGET_PREVIEW_POINTS`] values, each the mean of its
/// bucket; a bucket of gaps stays a gap.
fn downsample(values: &[Option<f64>]) -> Vec<Option<f64>> {
    let n = values.len();
    if n <= PROJECT_WIDGET_PREVIEW_POINTS {
        return values.to_vec();
    }
    (0..PROJECT_WIDGET_PREVIEW_POINTS)
        .map(|bucket| {
            let start = bucket * n / PROJECT_WIDGET_PREVIEW_POINTS;
            let end = ((bucket + 1) * n / PROJECT_WIDGET_PREVIEW_POINTS).max(start + 1);
            let found: Vec<f64> = values[start..end].iter().flatten().copied().collect();
            (!found.is_empty()).then(|| found.iter().sum::<f64>() / found.len() as f64)
        })
        .collect()
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dashboard_keeps_its_figures_chart_shape_and_table() {
        let script = r#"
            local days, events = {}, {}
            for d = 1, 90 do days[d] = tostring(d); events[d] = d end
            return ui.column {
              ui.row {
                ui.stat { label = "Events", value = "41,920", delta = "+8.2%" },
                ui.stat { label = "Sessions", value = "27,310" },
              },
              ui.chart { type = "bar", x = days, series = { { name = "Events", values = events } } },
              ui.table { columns = { "Page", "Views" }, rows = { { "/pricing", "8,214" } } },
            }
        "#;
        let preview = extract(script, 7, Default::default()).expect("a preview");
        assert_eq!(preview.stats.len(), 2);
        assert_eq!(preview.stats[0].delta.as_deref(), Some("+8.2%"));
        let chart = preview.chart.expect("a chart");
        assert_eq!(chart.kind, ProjectWidgetPreviewChartKind::Bar);
        let values = &chart.series[0].values;
        assert_eq!(values.len(), PROJECT_WIDGET_PREVIEW_POINTS);
        // Bars rise from zero, and the last bucket holds the highest days.
        assert!(values[0].unwrap() < 30);
        assert!(values.last().copied().flatten().unwrap() > 980);
        assert_eq!(preview.table.unwrap().rows[0], vec!["/pricing", "8,214"]);
        assert!(preview.text.is_none());

        assert!(
            extract("return ui.column {}", 1, Default::default())
                .unwrap()
                .is_empty()
        );
        assert!(extract("error('boom')", 1, Default::default()).is_none());
    }
}
